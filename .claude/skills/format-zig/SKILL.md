---
name: format-zig
description:
  "Format Zig files with zig fmt. Use after creating or editing any Zig file
  (.zig)."
---

# Format Zig

Run `zig fmt` on every Zig file after creating or editing it.

## When This Skill Applies

After every Write or Edit to any `.zig` file in the project. This includes:

- GUI source files (`gui/src/**/*.zig`)
- Build files (`gui/build.zig`)
- Any other Zig file in the project

## Process

After your edits to a Zig file are complete, run:

```bash
zig fmt <file_path>
```

That's it. No other steps needed.
