# BBDown protocol reference

The built-in Rust implementation was developed with reference to
[hst1189/BiliDown](https://github.com/hst1189/BiliDown), a fork of
[nilaoda/BBDown](https://github.com/nilaoda/BBDown), at commit
`259a5558cee0a349a7ebb60bd31e40c88e5bc1ed`.

The WBI mixin permutation, quality and codec mappings, WEB/TV/APP/INTL playback
parameters, TV signing keys, protobuf field definitions, QR login flows, subtitle
protocols, and list/episode metadata handling are adapted from that reference.
The C# executable, .NET runtime, and BBDown process are not bundled or invoked.
This implementation covers the four playback interfaces and paginated video lists.
It does not reproduce every upstream interface or feature.

Upstream license:

MIT License

Copyright (c) 2020 nilaoda

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
