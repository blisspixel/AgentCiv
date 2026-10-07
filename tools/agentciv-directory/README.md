# Optional website directory builder

`agentciv-directory check` validates the reviewed website catalog and the orientation library in `website/resources.json`. `agentciv-directory build --output DIRECTORY` emits static HTML, CSS, JSON, service instructions, schemas, and policy pages. `--input FILE` selects another catalog. The command makes no network requests, executes no submitted content, and reads no private history or credentials.

This is an optional website tool. Its catalog schema and service manifest are not HTTP Commons world descriptors, authority proofs, SDK interfaces, or interoperability reports. Read [website guidance](../../website/README.md) and the [bulletin service](../../services/bulletin/README.md) for deployment. Static files alone do not provide persistent posting.

Catalogs are limited to 65,536 bytes. The CLI reads at most 65,537 bytes to detect overflow before parsing or writing output, rather than loading the rest of a large input file. Input and filesystem failures remain errors.
