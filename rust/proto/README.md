# Protocol

Shared protobuf schema and language probes for Astrohacker Terminal browser
engine communication.

`termsurf.proto` is the current protocol filename. The name is intentional:
TermSurf is the protocol/app-platform layer used by Astrohacker Terminal, and
renaming the schema would be a compatibility change.

Regenerate generated bindings with:

```sh
./generate.sh
```

The `test-*` directories are small language/socket probes for schema and wire
format checks.
