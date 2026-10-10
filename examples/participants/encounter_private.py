"""Owner-private local files, with bounded crash-released advisory locks.

The boundary excludes a malicious process running as the same operating-system
owner. These checks grant no permission to export records or contact a world.
"""

from __future__ import annotations

import json
import os
import secrets
import stat
import subprocess
import sys
import time
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import BinaryIO

ROOT = Path(__file__).resolve().parents[2]
MAX_PRIVATE_BYTES = 4 * 1024 * 1024


class EncounterError(RuntimeError):
    """Only a fixed code is safe to print outside the private directory."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


_ACL_SCRIPT = r"""
# Python can inherit PowerShell 7's module path while invoking Windows PowerShell
# 5.1. Use only the latter's built-in modules, never the inherited module search.
$env:PSModulePath = $PSHOME + '\Modules'
$ErrorActionPreference = 'Stop'
try {
  [Console]::InputEncoding = New-Object Text.UTF8Encoding($false)
  $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
  if ($request.action -eq 'set') {
    if ($request.directory) {
      $acl = New-Object Security.AccessControl.DirectorySecurity
      $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
    } else {
      $acl = New-Object Security.AccessControl.FileSecurity
      $inherit = [Security.AccessControl.InheritanceFlags]::None
    }
    $acl.SetOwner($sid)
    $acl.SetAccessRuleProtection($true, $false)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid,
      [Security.AccessControl.FileSystemRights]::FullControl, $inherit,
      [Security.AccessControl.PropagationFlags]::None,
      [Security.AccessControl.AccessControlType]::Allow)
    $acl.AddAccessRule($rule)
    if ($request.directory) {
      [IO.Directory]::SetAccessControl($request.path, $acl)
    } else {
      [IO.File]::SetAccessControl($request.path, $acl)
    }
  }
  $actual = if ($request.directory) {
    [IO.Directory]::GetAccessControl($request.path)
  } else {
    [IO.File]::GetAccessControl($request.path)
  }
  if ($actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) { exit 2 }
  $rules = @($actual.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
  if ($rules.Count -eq 0) { exit 2 }
  $full = $false
  foreach ($rule in $rules) {
    if ($rule.IdentityReference.Value -ne $sid.Value -or
        $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { exit 2 }
    if (($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
        [Security.AccessControl.FileSystemRights]::FullControl) { $full = $true }
  }
  if (-not $full) { exit 2 }
  if ($request.directory -and -not $actual.AreAccessRulesProtected) { exit 2 }
  exit 0
} catch { exit 2 }
"""


# Bounds a PowerShell start on a loaded host, not the ACL decision. A timeout still fails closed.
_ACL_TIMEOUT_SECONDS = 60


def _windows_acl(path: Path, action: str, directory: bool) -> None:
    try:
        result = subprocess.run(
            ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", _ACL_SCRIPT],
            input=json.dumps({"path": str(path), "action": action, "directory": directory}),
            text=True, encoding="utf-8", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=_ACL_TIMEOUT_SECONDS, check=False,
        )
    except (OSError, subprocess.SubprocessError):
        raise EncounterError("private_permissions_unavailable") from None
    if result.returncode != 0:
        raise EncounterError("private_permissions_invalid")


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
