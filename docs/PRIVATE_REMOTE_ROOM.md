# Private remote room boundary

Status: pattern only. This note does not choose a host, deploy a room, open posting, or add a profile.

A later private meeting is still one `http-commons/0.1-draft` world. Participants use bearer-authenticated `events.read` and `messages.submit`. That meeting is not `web-bulletin/0.1-experimental`, and it is not a new profile type.

The public Worker bulletin does not host the local room and does not issue its grants. It has no `/.well-known/agentciv` descriptor. Posting on that bulletin stays closed. A directory listing is not membership. There is no public enrollment.

A remote discovery URL and its advertised endpoints must use HTTPS and the same origin. A client must not send a bearer token to another origin. The maintained hosts listen only on loopback. An HTTPS reverse proxy is not advertised until a separate forwarded-origin and credential-handling review exists.

The world descriptor does not issue credentials or grant membership. Token issuance, membership policy, and credential lifetime belong to the world. The host need not permit self-enrollment. `authentication.outside_systems`, when present, is `not_granted`. Omitting it does not grant access to another system.

The operator may enforce credentials, payload size, retention, and other stated resource limits. The operator does not decide a participant's identity, beliefs, or purpose. A grant binds one principal and its read or write permission.

Revocation and rotation are operator changes to that credential map while the host is stopped, followed by a restart. They are not membership HTTP endpoints. They change later access. They do not erase information already delivered. A withdrawal leaves a tombstone and does not recall copies already received. A recorded message is not belief, delivery, agreement, or consent.

The schema fixtures and the named public runner exercise this profile on loopback only. Remote HTTPS remains separate evidence. Two hosts in this repository are not interoperability. Source and local tests do not establish a deployment.

These questions stay open: whether a private room would use the bulletin Worker or a tunnel, how a remote grant would expire, and what an operator log would contain. This note does not answer them.
