# Sounda Unicode bitmap font

Sounda embeds a converted subset of GNU Unifont 18.0.01, under the SIL Open Font License 1.1. This subset is named Sounda Unicode Bitmap. Glyph designs are unchanged; the binary layout and repertoire are modified. Sounda application code does not inherit the font license.

Copyright (C) 1998-2026 Roman Czyborra, Paul Hardy, Qianqian Fang, Andrew Miller, Johnnie Weaver, David Corbett, Ælla Chiana Moskopp, Rebecca Bettencourt, Minseo Lee, Ho-Seok Ee, et al.

Source: https://unifoundry.com/pub/unifont/unifont-18.0.01/ . The font files are dual-licensed; this distribution selects OFL 1.1. The source tarball SHA-256 is `eab60847aac34c8768765cecc7821faf50de2636187b452b9b5fa50a12b00bc3`.

The 1,261,520-byte atlas has 31,538 sorted glyphs, 8x16 or 16x16 pixels. Latin, Polish and other accented Latin letters, Cyrillic and its extensions, hiragana, katakana, common Chinese/Japanese ideographs, CJK Extension A and compatibility forms are included. This is BMP coverage, not all Unicode: supplementary CJK extensions, emoji, variation-selector glyph variants and complex-script shaping are unsupported. Unsupported characters display a square; original UTF-8 SD paths remain intact. Common decomposed Latin/kana pairs compose before rendering.

Regenerate with `python3 scripts/build_unicode.py PATH/TO/unifont-18.0.01.hex`, then `cargo +stable fmt`. Python Unicode 16.0.0 generated the folding/composition tables. The exact repertoire, inputs and binary hash are recorded in `assets/fonts/manifest.json`. Font lookup and drawing need no SD reads, heap font cache or font parser on the DSi.

## SIL Open Font License 1.1

The SIL Open Font License version 1.1 is copied below, and is also
available with a FAQ at http://scripts.sil.org/OFL.


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
