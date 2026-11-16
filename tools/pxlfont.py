r"""Build docs/play/themes/fonts/pxl64.woff2, pxl64-text.woff2 and pxl80.woff2:
the "Classic" theme's type. The C64's own 8x8 character cell as a webfont; the
same glyphs set proportionally for reading (each as wide as its pixels, plus
one); and at half width, 4x8, as 80-column C64 software drew them (the Studio).

The glyphs are the PXL font by Retrofan (version 2.3), from the MEGA65 Open
ROMs' character ROM (bin/chargen_pxlfont_2.3.rom), where Retrofan let it be
included under the GNU LGPL 3.0 - not Commodore's character ROM. Its
lower/upper case half gives the letters, digits and punctuation, the other
half a few PETSCII shapes. What a C64 never had and code needs ({ } ~ ^ ` \)
and a few signs the pages use (play arrows, a star, a check) are drawn here in
the same hand: two-pixel strokes on rows 1-6, descenders on row 7.

Each pixel is a square of 128 units: an 8x8 cell is the em, so at 8, 16, 24 px
the type falls on whole screen pixels. Rows 0-6 stand above the baseline.

  python3 -m venv /tmp/fontenv && /tmp/fontenv/bin/pip install fonttools brotli
  /tmp/fontenv/bin/python tools/pxlfont.py [chargen.rom]

Without a ROM argument the ROM is downloaded from mega65.github.io/open-roms.
"""
import sys
import urllib.request
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

URL = 'https://mega65.github.io/open-roms/bin/chargen_pxlfont_2.3.rom'
OUT = Path(__file__).resolve().parent.parent / 'docs/play/themes/fonts/pxl64.woff2'
PX = 128
EM = 8 * PX

# screen codes of the lower/upper case set (the ROM's second 2 KB) by character
LOWER = {'@': 0, '[': 27, '£': 28, ']': 29, '↑': 30, '←': 31, '─': 64, '│': 93, '_': 100,
         '▒': 102, '├': 107, '└': 109, '┐': 110, '┌': 112, '┴': 113, '┬': 114, '┤': 115, '┘': 125,
         '▌': 97, '▄': 98, '▔': 99, '▁': 100, '✓': 122}
LOWER.update({chr(ord('a') + i): 1 + i for i in range(26)})
LOWER.update({chr(ord('A') + i): 65 + i for i in range(26)})
LOWER.update({chr(c): c for c in range(32, 64)})
# PETSCII shapes from the upper case/graphics set (the first 2 KB)
UPPER = {'♠': 65, '♥': 83, '♣': 88, '♦': 90, '●': 81, '○': 87, 'π': 94, '╳': 86, '╭': 85, '╮': 73, '╰': 74, '╯': 75}

# drawn here, eight rows of eight
DRAWN = {
    '{': ['........', '....###.', '...##...', '.###....', '...##...', '...##...', '....###.', '........'],
    '}': ['........', '.###....', '...##...', '....###.', '...##...', '...##...', '.###....', '........'],
    '\\': ['........', '..##....', '..##....', '...##...', '...##...', '....##..', '....##..', '........'],
    '~': ['........', '........', '........', '.###.##.', '##.###..', '........', '........', '........'],
    '^': ['........', '...##...', '..####..', '.##..##.', '........', '........', '........', '........'],
    '`': ['........', '..##....', '...##...', '........', '........', '........', '........', '........'],
    '·': ['........', '........', '........', '...##...', '...##...', '........', '........', '........'],
    '•': ['........', '........', '..####..', '..####..', '..####..', '..####..', '........', '........'],
    '…': ['........', '........', '........', '........', '........', '##.##.##', '##.##.##', '........'],
    '–': ['........', '........', '........', '........', '.######.', '........', '........', '........'],
    '—': ['........', '........', '........', '........', '########', '........', '........', '........'],
    '▶': ['.#......', '.###....', '.#####..', '.#######', '.#####..', '.###....', '.#......', '........'],
    '◀': ['......#.', '....###.', '..#####.', '#######.', '..#####.', '....###.', '......#.', '........'],
    '▲': ['........', '...##...', '..####..', '.######.', '########', '........', '........', '........'],
    '▼': ['........', '........', '########', '.######.', '..####..', '...##...', '........', '........'],
    '→': ['........', '....#...', '....##..', '#######.', '....##..', '....#...', '........', '........'],
    '↓': ['........', '...##...', '...##...', '...##...', '.######.', '..####..', '...##...', '........'],
    '✕': ['........', '.##..##.', '..####..', '...##...', '..####..', '.##..##.', '........', '........'],
    '★': ['...##...', '..####..', '########', '.######.', '..####..', '.##..##.', '.#....#.', '........'],
    '☰': ['........', '.######.', '........', '.######.', '........', '.######.', '........', '........'],
    '»': ['........', '........', '##..##..', '.##..##.', '..##..##', '.##..##.', '##..##..', '........'],
    '«': ['........', '........', '..##..##', '.##..##.', '##..##..', '.##..##.', '..##..##', '........'],
    '█': ['########'] * 8,
}
# shapes that fill or join across the cell: whole cells in the proportional cut too
BLOCKS = set('─│┌┐└┘├┤┬┴▒█▌▄▔▁_—…')
# the same glyph under another code point
ALIAS = {'‹': '<', '›': '>', '‘': "'", '’': "'", '“': '"', '”': '"', ' ': ' ', '×': '✕', '⋯': '…', '▸': '▶', '◂': '◀'}


def rows_of(rom, half, code):
    b = rom[half * 2048 + code * 8: half * 2048 + code * 8 + 8]
    return [format(r, '08b').replace('0', '.').replace('1', '#') for r in b]


def glyph(rows):
    """The pixels as outlines: one rectangle per run of set pixels in a row."""
    pen = TTGlyphPen(None)
    for y, row in enumerate(rows):
        top = (7 - y) * PX  # row 0 is the top; the baseline is under row 6
        x = 0
        while x < len(row):
            if row[x] != '#':
                x += 1
                continue
            end = x
            while end < len(row) and row[end] == '#':
                end += 1
            # clockwise, as TrueType fills
            pen.moveTo((x * PX, top - PX))
            pen.lineTo((x * PX, top))
            pen.lineTo((end * PX, top))
            pen.lineTo((end * PX, top - PX))
            pen.closePath()
            x = end
    return pen.glyph()


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else None
    rom = Path(src).read_bytes() if src else urllib.request.urlopen(URL).read()
    assert len(rom) == 4096, 'a character ROM is 4 KB'
    shapes = {ch: rows_of(rom, 1, c) for ch, c in LOWER.items()}
    shapes.update({ch: rows_of(rom, 0, c) for ch, c in UPPER.items()})
    shapes.update(DRAWN)
    shapes.update({a: shapes[b] for a, b in ALIAS.items()})

    build(shapes, 'PXL 64', OUT, proportional=False)
    build(shapes, 'PXL 64 Text', OUT.with_name('pxl64-text.woff2'), proportional=True)
    halves = {ch: half(rows, ch in BLOCKS) for ch, rows in shapes.items()}
    halves.update(HALF_DRAWN)
    build(halves, 'PXL 80', OUT.with_name('pxl80.woff2'), proportional=False, width=4)


# the half-width glyphs merging can't make (diagonals, three stems in three pixels): drawn here, 4x8, the 4th column the gap
HALF_DRAWN = {
    '/': ['....', '..#.', '..#.', '.#..', '.#..', '#...', '#...', '....'],
    '\\': ['....', '#...', '#...', '.#..', '.#..', '..#.', '..#.', '....'],
    '%': ['....', '#.#.', '..#.', '.#..', '.#..', '#...', '#.#.', '....'],
    '&': ['....', '.#..', '#.#.', '.#..', '###.', '#.#.', '.##.', '....'],
    '#': ['....', '#.#.', '###.', '#.#.', '#.#.', '###.', '#.#.', '....'],
    '$': ['.#..', '###.', '#...', '###.', '..#.', '###.', '.#..', '....'],
    '*': ['....', '....', '#.#.', '.#..', '###.', '.#..', '#.#.', '....'],
    '~': ['....', '....', '....', '.##.', '##..', '....', '....', '....'],
    '0': ['....', '.#..', '#.#.', '#.#.', '#.#.', '#.#.', '.#..', '....'],
    '(': ['....', '.#..', '#...', '#...', '#...', '#...', '.#..', '....'],
    ')': ['....', '.#..', '..#.', '..#.', '..#.', '..#.', '.#..', '....'],
    '[': ['....', '##..', '#...', '#...', '#...', '#...', '##..', '....'],
    ']': ['....', '##..', '.#..', '.#..', '.#..', '.#..', '##..', '....'],
    'M': ['....', '#.#.', '###.', '###.', '#.#.', '#.#.', '#.#.', '....'],
    'N': ['....', '##..', '#.#.', '#.#.', '#.#.', '#.#.', '#.#.', '....'],
    'R': ['....', '##..', '#.#.', '#.#.', '##..', '#.#.', '#.#.', '....'],
    'W': ['....', '#.#.', '#.#.', '#.#.', '###.', '###.', '#.#.', '....'],
    'X': ['....', '#.#.', '#.#.', '.#..', '.#..', '#.#.', '#.#.', '....'],
    'm': ['....', '....', '....', '###.', '###.', '#.#.', '#.#.', '....'],
    'w': ['....', '....', '....', '#.#.', '#.#.', '###.', '###.', '....'],
    'x': ['....', '....', '....', '#.#.', '.#..', '.#..', '#.#.', '....'],
    'k': ['....', '#...', '#...', '#.#.', '##..', '##..', '#.#.', '....'],
}


def half(rows, block):
    """A glyph at half width, as 80-column C64 software drew its 4x8 characters in bitmap mode:
    PXL's strokes are two pixels wide from an odd column, so each pair (1-2, 3-4, 5-6) becomes
    one column and the fourth is the gap; a shape that fills the cell pairs from column 0."""
    start = 0 if block else 1
    cols = range(start, start + 8 - (0 if block else 2), 2)
    return [''.join('#' if '#' in r[c:c + 2] else '.' for c in cols).ljust(4, '.')[:4] for r in rows]


def ink(rows):
    """The first and last column a glyph sets, or None for a blank one."""
    cols = [x for x in range(len(rows[0])) if any(r[x] == "#" for r in rows)]
    return (cols[0], cols[-1]) if cols else None


def build(shapes, family, out, proportional, width=8):
    """The font: monospaced in the C64's 8-pixel cell, or (for reading) each glyph only as
    wide as its pixels and one pixel of space - still whole pixels, still on the grid."""
    names = ['.notdef'] + [f'u{ord(ch):04X}' for ch in shapes]
    metrics, glyphs = {'.notdef': (width * PX, 0)}, {}
    glyphs['.notdef'] = glyph(['#' * width] + ['#' + '.' * (width - 2) + '#'] * 5 + ['#' * width, '.' * width])
    for ch, rows in shapes.items():
        name = f'u{ord(ch):04X}'
        span = ink(rows)
        if proportional and span and not ch.isspace() and ch not in BLOCKS:
            first, last = span
            rows = [r[first:] + '.' * first for r in rows]  # flush left: the gap is the advance's last pixel
            metrics[name] = ((last - first + 2) * PX, 0)
        elif proportional and ch.isspace():
            metrics[name] = (4 * PX, 0)  # a word space of half a cell
        else:
            metrics[name] = (width * PX, 0)
        glyphs[name] = glyph(rows)
    fb = FontBuilder(EM, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({ord(ch): f'u{ord(ch):04X}' for ch in shapes})
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=7 * PX, descent=-PX)
    fb.setupNameTable({
        'familyName': family, 'styleName': 'Regular',
        'copyright': 'PXL font by Retrofan; from the MEGA65 Open ROMs (GNU LGPL 3.0)',
        'licenseDescription': 'GNU Lesser General Public License 3.0', 'licenseInfoURL': 'https://www.gnu.org/licenses/lgpl-3.0.html',
    })
    fb.setupOS2(sTypoAscender=7 * PX, sTypoDescender=-PX, sTypoLineGap=0, usWinAscent=7 * PX, usWinDescent=PX,
                sxHeight=5 * PX, sCapHeight=6 * PX, fsType=0)
    fb.setupPost(isFixedPitch=0 if proportional else 1)
    fb.font.flavor = 'woff2'
    out.parent.mkdir(parents=True, exist_ok=True)
    fb.save(out)
    print(f'{out}: {family}, {len(shapes)} characters, {out.stat().st_size} bytes')


if __name__ == '__main__':
    main()
