# Third-party notices

JARVIG's own license is not chosen. The notices below cover software this repository actually depends on or names as an upstream.

## Toolchain

TypeScript, Vitest, tsx, and `@types/node` are installed as development dependencies. Their license texts are in `node_modules` after install and are not copied here verbatim. Versions are in `dependency-manifest.json`. Before a binary distribution, collect the transitive notice set with a dedicated license scan and commit that scan's output.

## PlayCanvas Engine

Not included in this repository.

```text
Copyright (c) 2011-2026 PlayCanvas Ltd.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

The notice above is the MIT license text published with PlayCanvas Engine. It applies only if and when that software is copied into this repository.

## koffi

Host-only dependency, MIT, version 2.16.3. Copyright Niels Martignène. Used so Node hosts can call the native core C ABI. It is not part of the engine runtime. The license text ships inside `node_modules/koffi` after install.

## CryEngine

No CryEngine files are included. No Crytek notice is implied.
