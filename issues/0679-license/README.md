+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 679: License and Trademark

Set up licensing for TermSurf. The software should be open source, but the
"TermSurf" brand (name, logo, icons) must be protected. Copyright belongs to
Identellica LLC.

## Background

### Upstream Licenses

TermSurf builds on two major open-source projects. Both use permissive licenses.

**Ghostty — MIT License**

The GUI (`gui/`) is a Ghostty fork. Ghostty uses the MIT License:

```
MIT License

Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

MIT obligations:

- Include the original copyright notice and license in all copies or substantial
  portions
- That's it — no other restrictions

**Chromium — BSD 3-Clause License**

The browser engine (`chromium/`) is a Chromium fork. Chromium uses BSD 3-Clause:

```
Copyright 2015 The Chromium Authors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

   * Redistributions of source code must retain the above copyright notice,
     this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above copyright notice,
     this list of conditions and the following disclaimer in the documentation
     and/or other materials provided with the distribution.
   * Neither the name of Google LLC nor the names of its contributors may be
     used to endorse or promote products derived from this software without
     specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

BSD 3-Clause obligations:

- Retain copyright notice in source distributions
- Reproduce copyright notice in binary distributions
- Do not use "Google" or contributor names to endorse the product

**Chromium's trademark precedent:** Google explicitly separates the open-source
code (BSD) from the "Chrome" trademark. The Chrome name, logo, and branded
assets are NOT open source. Chromium (the open-source project) has its own
branding. This is the same model TermSurf should follow.

### Other Upstream Dependencies

- **Nerd Fonts** (`gui/vendor/nerd-fonts/`) — SIL Open Font License v1.1 (fonts)
  - MIT (source code). Compatible with MIT.
- **Rust crates** (`tui/`) — Various permissive licenses (MIT, Apache 2.0, BSD).
  All compatible with MIT.
- **Zig packages** (`gui/build.zig.zon`) — Various permissive licenses. All
  compatible with MIT.

### License Compatibility

MIT and BSD 3-Clause are both permissive and fully compatible. TermSurf can use
MIT for its own code while satisfying both upstream obligations. The only
requirement is to include the original copyright notices from Ghostty and
Chromium.

### Trademark Strategy

Many open-source projects separate code licenses from brand protection:

- **Chromium/Chrome** — Code is BSD, "Chrome" is a Google trademark
- **Firefox/Mozilla** — Code is MPL, "Firefox" is a Mozilla trademark
- **Rust** — Code is MIT/Apache 2.0, "Rust" is a Rust Foundation trademark
- **Linux** — Code is GPL, "Linux" is a Linus Torvalds trademark

TermSurf should follow this pattern: MIT license for code, trademark protection
for the "TermSurf" name and logo.

### What We Need

1. **`LICENSE`** at repo root — MIT License, Copyright Identellica LLC
2. **`TRADEMARKS.md`** at repo root — reserves "TermSurf" name and logo
3. **`NOTICE`** at repo root — third-party copyright notices (Ghostty, Chromium)
4. Keep `gui/LICENSE` intact (Ghostty's original MIT license)

## Experiment 1: Create license files

### Hypothesis

Adding LICENSE, TRADEMARKS.md, and NOTICE files will properly license the
software as open source while protecting the TermSurf brand.

### Changes

#### 1. Create `LICENSE`

MIT License with Identellica LLC copyright. Year 2025 (project inception).

#### 2. Create `TRADEMARKS.md`

Reserve "TermSurf", the TermSurf logo, and related branding. Clarify that forks
must use a different name. This is a policy document, not a legal instrument —
actual trademark registration is separate.

#### 3. Create `NOTICE`

Third-party attributions required by upstream licenses:

- Ghostty MIT notice (required by MIT: "shall be included in all copies or
  substantial portions")
- Chromium BSD notice (required by BSD: "reproduce the above copyright notice")

#### 4. Keep `gui/LICENSE` unchanged

Ghostty's original MIT license stays in place inside the fork directory.

### Test

1. `LICENSE` exists at repo root with MIT + Identellica LLC
2. `TRADEMARKS.md` exists with brand protection language
3. `NOTICE` exists with Ghostty and Chromium attributions
4. `gui/LICENSE` is unchanged (still Ghostty's MIT)
5. No conflicts between TermSurf's MIT and upstream licenses

### Result: PASS

All three files created. `LICENSE` has MIT with Identellica LLC copyright.
`TRADEMARKS.md` reserves the TermSurf name and logo with clear permitted/not
permitted uses. `NOTICE` includes full Ghostty MIT and Chromium BSD 3-Clause
copyright texts. `gui/LICENSE` unchanged.

## Conclusion

TermSurf is now properly licensed. MIT for the code, trademark protection for
the brand — the same model used by Chromium/Chrome, Firefox/Mozilla, and Rust.

- `LICENSE` — MIT, Copyright (c) 2025 Identellica LLC
- `TRADEMARKS.md` — "TermSurf" name and logo reserved, forks must rebrand
- `NOTICE` — Ghostty and Chromium upstream attributions
