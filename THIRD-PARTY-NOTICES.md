# Third-party notices

Mamá Cine's own code is under the MIT license in `LICENSE`. What travels with it is not, and this file says what that is and under which terms.

## Programs the app drives

These are separate programs, run as their own processes. They are not linked into the app, and the MIT license does not cover them. The Windows installer bundles them because Windows ships with none of them; on Linux they come from the system. Each is fetched from its upstream release and verified against a recorded checksum by `scripts/fetch-sidecars.sh`.

- [nzbget](https://github.com/nzbgetcom/nzbget), GPL-2.0-or-later. The bundled build is the unmodified upstream release; its corresponding source is published at the same release tag.
- [UnRAR](https://www.rarlab.com/rar_add.htm), under the UnRAR license, which permits free distribution of the unmodified utility.
- [7-Zip](https://www.7-zip.org/) (`7za`), LGPL-2.1 with the unRAR restriction, some code under BSD 3-clause. Source at [7-zip.org](https://www.7-zip.org/download.html).

## Vendored interface libraries

`ui/vendor/htm-preact.js` is the standalone bundle of two MIT-licensed libraries:

- [Preact](https://github.com/preactjs/preact), copyright Jason Miller.
- [htm](https://github.com/developit/htm), copyright Google LLC.

## Fonts

Both under the [SIL Open Font License 1.1](https://openfontlicense.org/):

- [Atkinson Hyperlegible](https://www.brailleinstitute.org/freefont/), copyright Braille Institute of America.
- [Fraunces](https://github.com/undercasetype/Fraunces), copyright The Fraunces Project Authors.
