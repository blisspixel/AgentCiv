"""Owner-private local files, with bounded crash-released advisory locks.

The boundary excludes a malicious process running as the same operating-system
owner. These checks grant no permission to export records or contact a world.
"""

from __future__ import annotations

import ctypes
import json
import os
import secrets
import stat
import subprocess
import sys
import threading
import time
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import Any, BinaryIO

ROOT = Path(__file__).resolve().parents[2]
MAX_PRIVATE_BYTES = 4 * 1024 * 1024


class EncounterError(RuntimeError):
    """Only fixed codes and caller-owned field names leave the private directory."""

    def __init__(self, code: str, *, name: str | None = None, field: str | None = None) -> None:
        super().__init__(code)
        self.code = code
        # Optional location relative to the private state directory, never an absolute path.
        self.name = name
        self.field = field


_TOKEN_QUERY = 0x0008
_TOKEN_USER_CLASS = 1
_ERROR_SUCCESS = 0
_SE_FILE_OBJECT = 1
_OWNER_SECURITY_INFORMATION = 0x00000001
_DACL_SECURITY_INFORMATION = 0x00000004
_PROTECTED_DACL_SECURITY_INFORMATION = 0x80000000
_ACL_REVISION = 2
_ACCESS_ALLOWED_ACE_TYPE = 0
_OBJECT_INHERIT_ACE = 0x01
_CONTAINER_INHERIT_ACE = 0x02
_INHERIT_ONLY_ACE = 0x08
_DIR_INHERIT_FLAGS = _CONTAINER_INHERIT_ACE | _OBJECT_INHERIT_ACE
_FILE_ALL_ACCESS = 0x001F01FF
_SE_DACL_PROTECTED = 0x1000
_ACL_SIZE_INFORMATION_CLASS = 2
_ACL_HEADER_BYTES = 8


class _SidAndAttributes(ctypes.Structure):
    _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", ctypes.c_uint32)]


class _AclSizeInformation(ctypes.Structure):
    _fields_ = [
        ("AceCount", ctypes.c_uint32),
        ("AclBytesInUse", ctypes.c_uint32),
        ("AclBytesFree", ctypes.c_uint32),
    ]


class _AccessAllowedAceHeader(ctypes.Structure):
    _fields_ = [
        ("AceType", ctypes.c_uint8),
        ("AceFlags", ctypes.c_uint8),
        ("AceSize", ctypes.c_uint16),
        ("Mask", ctypes.c_uint32),
    ]


def _bind_win32(library: Any, name: str, argtypes: list[Any], restype: Any) -> Callable[..., Any]:
    function = getattr(library, name)
    function.argtypes = argtypes
    function.restype = restype
    return function  # type: ignore[no-any-return]


class _Win32AclApi:
    """Direct Win32 advapi32/kernel32 owner-only ACL enforcement without subprocesses."""

    def __init__(self, advapi32: Any, kernel32: Any) -> None:
        void_p = ctypes.c_void_p
        p_void_p = ctypes.POINTER(ctypes.c_void_p)
        u16 = ctypes.c_uint16
        u32 = ctypes.c_uint32
        p_u16 = ctypes.POINTER(ctypes.c_uint16)
        p_u32 = ctypes.POINTER(ctypes.c_uint32)
        c_bool = ctypes.c_int

        self._get_current_process = _bind_win32(kernel32, "GetCurrentProcess", [], void_p)
        self._close_handle = _bind_win32(kernel32, "CloseHandle", [void_p], c_bool)
        self._local_free = _bind_win32(kernel32, "LocalFree", [void_p], void_p)

        self._open_process_token = _bind_win32(advapi32, "OpenProcessToken", [void_p, u32, p_void_p], c_bool)
        self._get_token_information = _bind_win32(advapi32, "GetTokenInformation", [void_p, u32, void_p, u32, p_u32], c_bool)
        self._is_valid_sid = _bind_win32(advapi32, "IsValidSid", [void_p], c_bool)
        self._get_length_sid = _bind_win32(advapi32, "GetLengthSid", [void_p], u32)
        self._copy_sid = _bind_win32(advapi32, "CopySid", [u32, void_p, void_p], c_bool)
        self._equal_sid = _bind_win32(advapi32, "EqualSid", [void_p, void_p], c_bool)
        self._initialize_acl = _bind_win32(advapi32, "InitializeAcl", [void_p, u32, u32], c_bool)
        self._add_access_allowed_ace_ex = _bind_win32(advapi32, "AddAccessAllowedAceEx", [void_p, u32, u32, u32, void_p], c_bool)
        self._set_named_security_info = _bind_win32(
            advapi32, "SetNamedSecurityInfoW", [ctypes.c_wchar_p, u32, u32, void_p, void_p, void_p, void_p], u32,
        )
        self._get_named_security_info = _bind_win32(
            advapi32, "GetNamedSecurityInfoW",
            [ctypes.c_wchar_p, u32, u32, p_void_p, p_void_p, p_void_p, p_void_p, p_void_p], u32,
        )
        self._get_security_descriptor_control = _bind_win32(
            advapi32, "GetSecurityDescriptorControl", [void_p, p_u16, p_u32], c_bool,
        )
        self._get_acl_information = _bind_win32(advapi32, "GetAclInformation", [void_p, void_p, u32, u32], c_bool)
        self._get_ace = _bind_win32(advapi32, "GetAce", [void_p, u32, p_void_p], c_bool)
        self._user_sid_buf, self._user_sid_len = self._resolve_user_sid()

    def _resolve_user_sid(self) -> tuple[ctypes.Array[ctypes.c_char], int]:
        token = ctypes.c_void_p()
        if not self._open_process_token(self._get_current_process(), _TOKEN_QUERY, ctypes.byref(token)) or not token.value:
            raise EncounterError("private_permissions_unavailable")
        try:
            needed = ctypes.c_uint32(0)
            self._get_token_information(token, _TOKEN_USER_CLASS, None, 0, ctypes.byref(needed))
            if needed.value == 0:
                raise EncounterError("private_permissions_unavailable")
            token_buf = ctypes.create_string_buffer(needed.value)
            if not self._get_token_information(token, _TOKEN_USER_CLASS, token_buf, needed.value, ctypes.byref(needed)):
                raise EncounterError("private_permissions_unavailable")
            sid_ptr = _SidAndAttributes.from_buffer(token_buf).Sid
            if not sid_ptr or not self._is_valid_sid(sid_ptr):
                raise EncounterError("private_permissions_unavailable")
            sid_len = int(self._get_length_sid(sid_ptr))
            if sid_len <= 0:
                raise EncounterError("private_permissions_unavailable")
            sid_buf = ctypes.create_string_buffer(sid_len)
            if not self._copy_sid(sid_len, ctypes.c_void_p(ctypes.addressof(sid_buf)), sid_ptr):
                raise EncounterError("private_permissions_unavailable")
            return sid_buf, sid_len
        finally:
            self._close_handle(token)

    def _user_sid_ptr(self) -> ctypes.c_void_p:
        return ctypes.c_void_p(ctypes.addressof(self._user_sid_buf))

    def set_owner_only(self, path: Path, directory: bool) -> None:
        ace_flags = _DIR_INHERIT_FLAGS if directory else 0
        acl_size = ((_ACL_HEADER_BYTES + ctypes.sizeof(_AccessAllowedAceHeader) + self._user_sid_len + 3) // 4) * 4
        dacl_buf = ctypes.create_string_buffer(acl_size)
        dacl_ptr = ctypes.c_void_p(ctypes.addressof(dacl_buf))
        user_sid = self._user_sid_ptr()
        if not self._initialize_acl(dacl_ptr, acl_size, _ACL_REVISION):
            raise EncounterError("private_permissions_unavailable")
        if not self._add_access_allowed_ace_ex(dacl_ptr, _ACL_REVISION, ace_flags, _FILE_ALL_ACCESS, user_sid):
            raise EncounterError("private_permissions_unavailable")
        security_info = _OWNER_SECURITY_INFORMATION | _DACL_SECURITY_INFORMATION | _PROTECTED_DACL_SECURITY_INFORMATION
        status = int(self._set_named_security_info(str(path), _SE_FILE_OBJECT, security_info, user_sid, None, dacl_ptr, None))
        if status != _ERROR_SUCCESS:
            raise EncounterError("private_permissions_unavailable")

    def check_owner_only(self, path: Path, directory: bool) -> None:
        owner_sid = ctypes.c_void_p()
        dacl = ctypes.c_void_p()
        sd = ctypes.c_void_p()
        security_info = _OWNER_SECURITY_INFORMATION | _DACL_SECURITY_INFORMATION
        status = int(self._get_named_security_info(
            str(path), _SE_FILE_OBJECT, security_info, ctypes.byref(owner_sid), None, ctypes.byref(dacl), None, ctypes.byref(sd),
        ))
        try:
            if status != _ERROR_SUCCESS or not sd.value:
                raise EncounterError("private_permissions_unavailable")
            user_sid = self._user_sid_ptr()
            if not owner_sid.value or not self._equal_sid(owner_sid, user_sid):
                raise EncounterError("private_permissions_invalid")
            if not dacl.value:
                raise EncounterError("private_permissions_invalid")
            if directory:
                control = ctypes.c_uint16(0)
                revision = ctypes.c_uint32(0)
                if not self._get_security_descriptor_control(sd, ctypes.byref(control), ctypes.byref(revision)):
                    raise EncounterError("private_permissions_unavailable")
                if (control.value & _SE_DACL_PROTECTED) == 0:
                    raise EncounterError("private_permissions_invalid")
            acl_info = _AclSizeInformation()
            if not self._get_acl_information(dacl, ctypes.byref(acl_info), ctypes.sizeof(acl_info), _ACL_SIZE_INFORMATION_CLASS):
                raise EncounterError("private_permissions_unavailable")
            if acl_info.AceCount == 0:
                raise EncounterError("private_permissions_invalid")
            full = False
            ace_header_bytes = ctypes.sizeof(_AccessAllowedAceHeader)
            for index in range(acl_info.AceCount):
                ace_ptr = ctypes.c_void_p()
                if not self._get_ace(dacl, index, ctypes.byref(ace_ptr)) or not ace_ptr.value:
                    raise EncounterError("private_permissions_unavailable")
                header = _AccessAllowedAceHeader.from_address(ace_ptr.value)
                ace_sid = ctypes.c_void_p(ace_ptr.value + ace_header_bytes)
                if header.AceType != _ACCESS_ALLOWED_ACE_TYPE or not self._equal_sid(ace_sid, user_sid):
                    raise EncounterError("private_permissions_invalid")
                has_full_rights = (header.Mask & _FILE_ALL_ACCESS) == _FILE_ALL_ACCESS
                has_inherit = (not directory) or (header.AceFlags & _DIR_INHERIT_FLAGS) == _DIR_INHERIT_FLAGS
                if has_full_rights and has_inherit and (header.AceFlags & _INHERIT_ONLY_ACE) == 0:
                    full = True
            if not full:
                raise EncounterError("private_permissions_invalid")
        finally:
            if sd.value:
                self._local_free(sd)


_WIN32_ACL_LOCK = threading.Lock()
_WIN32_ACL_API: _Win32AclApi | None = None


def _get_win32_acl_api() -> _Win32AclApi:
    global _WIN32_ACL_API
    if _WIN32_ACL_API is not None:
        return _WIN32_ACL_API
    with _WIN32_ACL_LOCK:
        if _WIN32_ACL_API is not None:
            return _WIN32_ACL_API
        win_dll = getattr(ctypes, "WinDLL", None)
        if win_dll is None:
            raise EncounterError("private_permissions_unavailable")
        try:
            api = _Win32AclApi(win_dll("advapi32"), win_dll("kernel32"))
        except (OSError, AttributeError, ValueError, ctypes.ArgumentError):
            raise EncounterError("private_permissions_unavailable") from None
        _WIN32_ACL_API = api
        return api


def _windows_acl(path: Path, action: str, directory: bool) -> None:
    if action not in ("set", "check"):
        raise EncounterError("private_permissions_invalid")
    try:
        api = _get_win32_acl_api()
        if action == "set":
            api.set_owner_only(path, directory)
        api.check_owner_only(path, directory)
    except EncounterError:
        raise
    except (OSError, AttributeError, ValueError, ctypes.ArgumentError):
        raise EncounterError("private_permissions_unavailable") from None


def find_executable(name: str) -> Path | None:
    """Search only absolute PATH entries for a tool that will receive private configuration.

    On Windows, shutil.which in Python 3.11 tries the current directory before PATH, so a
    planted binary there would run with the operator's credentials file.
    """
    suffixes = (".exe",) if sys.platform == "win32" else ("",)
    for entry in os.get_exec_path():
        if not entry or not os.path.isabs(entry):
            continue
        for suffix in suffixes:
            candidate = Path(entry) / f"{name}{suffix}"
            if candidate.is_file() and (sys.platform == "win32" or os.access(candidate, os.X_OK)):
                return candidate
    return None


def find_tool(name: str) -> Path | None:
    """Prefer absolute PATH entries, then this checkout's Cargo debug output."""
    found = find_executable(name)
    if found is not None:
        return found
    cargo = find_executable("cargo")
    if cargo is None:
        return None
    try:
        result = subprocess.run([str(cargo), "metadata", "--locked", "--offline", "--no-deps",
            "--format-version", "1"], cwd=ROOT, env={**os.environ, "RUSTUP_AUTO_INSTALL": "0"},
            capture_output=True, timeout=15, check=False)
        if result.returncode != 0 or len(result.stdout) > 1048576:
            return None
        target = json.loads(result.stdout).get("target_directory")
        if not isinstance(target, str) or not Path(target).is_absolute():
            return None
        candidate = Path(target) / "debug" / (name + (".exe" if sys.platform == "win32" else ""))
        return candidate if candidate.is_file() and (sys.platform == "win32" or os.access(candidate, os.X_OK)) else None
    except (OSError, ValueError, AttributeError, subprocess.SubprocessError):
        return None

def _path(path: Path) -> Path:
    if not path.is_absolute() or path.drive.startswith("\\\\"):
        raise EncounterError("private_path_invalid")
    normalized = Path(os.path.abspath(path))
    if normalized == ROOT or ROOT in normalized.parents:
        raise EncounterError("private_path_in_checkout")
    # Check every existing ancestor without following a link or Windows reparse point.
    for item in (*reversed(normalized.parents), normalized):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        except OSError:
            raise EncounterError("private_path_invalid") from None
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise EncounterError("private_path_link")
    return normalized


def _permissions(path: Path, directory: bool) -> None:
    if sys.platform == "win32":
        _windows_acl(path, "check", directory)
    else:
        info = path.stat(follow_symlinks=False)
        if info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != (0o700 if directory else 0o600):
            raise EncounterError("private_permissions_invalid")


def check_directory(path: Path) -> None:
    try:
        path = _path(path)
        if not stat.S_ISDIR(path.lstat().st_mode):
            raise EncounterError("private_directory_invalid")
        _permissions(path, True)
    except OSError:
        raise EncounterError("private_directory_invalid") from None


def check_file(path: Path) -> None:
    try:
        path = _path(path)
        check_directory(path.parent)
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise EncounterError("private_file_invalid")
        _permissions(path, False)
    except OSError:
        raise EncounterError("private_file_invalid") from None


def create_directory(path: Path) -> None:
    try:
        path = _path(path)
        # Existing parent directories are not changed. No recursive creation or cleanup.
        path.mkdir(mode=0o700)
        if sys.platform == "win32":
            _windows_acl(path, "set", True)
        else:
            path.chmod(0o700)
        check_directory(path)
    except FileExistsError:
        raise EncounterError("private_path_exists") from None
    except OSError:
        raise EncounterError("private_directory_failed") from None


def _open_existing(path: Path) -> BinaryIO:
    check_file(path)
    try:
        descriptor = os.open(path, os.O_RDWR | getattr(os, "O_NOFOLLOW", 0))
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            os.close(descriptor)
            raise EncounterError("private_file_invalid")
        return os.fdopen(descriptor, "r+b")
    except OSError:
        raise EncounterError("private_file_failed") from None


def read_bounded(path: Path, maximum: int) -> bytes:
    if type(maximum) is not int or not 0 < maximum <= MAX_PRIVATE_BYTES:
        raise EncounterError("private_bound_invalid")
    with _open_existing(path) as handle:
        value = handle.read(maximum + 1)
    if len(value) > maximum:
        raise EncounterError("private_file_too_large")
    return value


def _sync_directory(path: Path) -> None:
    if sys.platform != "win32":
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def write_new(path: Path, data: bytes) -> None:
    if not isinstance(data, bytes) or len(data) > MAX_PRIVATE_BYTES:
        raise EncounterError("private_bound_invalid")
    path = _path(path)
    check_directory(path.parent)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
        with os.fdopen(descriptor, "wb") as handle:
            # Parent ACL already excludes other identities before the first secret byte.
            if sys.platform == "win32":
                _windows_acl(path, "set", False)
            else:
                os.fchmod(handle.fileno(), 0o600)
            check_file(path)
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        _sync_directory(path.parent)
    except FileExistsError:
        raise EncounterError("private_path_exists") from None
    except OSError:
        raise EncounterError("private_write_failed") from None


def replace_private(path: Path, data: bytes) -> None:
    check_file(path)
    temporary = path.with_name(".replace-" + secrets.token_hex(12))
    try:
        write_new(temporary, data)
        os.replace(temporary, path)
        _sync_directory(path.parent)
    except OSError:
        raise EncounterError("private_write_failed") from None
    finally:
        if temporary.exists():
            temporary.unlink()


def _try_lock(handle: BinaryIO) -> bool:
    try:
        if sys.platform == "win32":
            import msvcrt
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False


def _unlock(handle: BinaryIO) -> None:
    if sys.platform == "win32":
        import msvcrt
        handle.seek(0)
        msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


@contextmanager
def lock(directory: Path, *, timeout: float = 2.0) -> Iterator[None]:
    if not 0 <= timeout <= 30:
        raise EncounterError("private_bound_invalid")
    check_directory(directory)
    path = directory / ".encounter.lock"
    try:
        write_new(path, b"0")
    except EncounterError as error:
        if error.code != "private_path_exists":
            raise
    with _open_existing(path) as handle:
        deadline = time.monotonic() + timeout
        while not _try_lock(handle):
            if time.monotonic() >= deadline:
                raise EncounterError("private_lock_unavailable")
            time.sleep(min(0.05, max(0, deadline - time.monotonic())))
        try:
            yield
        finally:
            _unlock(handle)
