r"""Build docs/play/themes/fonts/pumpernickel.woff2: the "Cupertino" theme's
type, a bold bitmap face drawn here for breadbin, in the manner of the system
type of the first desktops of 1984 (not a copy of any): two-pixel stems,
one-pixel joins, a 12-pixel body - capitals 9 pixels tall, lower case 6,
descenders 2 - each glyph as wide as its pixels plus one pixel of space.

Each pixel is a square of 100 units and the em is 12 of them, so at 12, 24,
36 px the type falls on whole screen pixels. Row 0 is the top of the body,
rows 1-9 stand above the baseline, rows 10-11 hang below it.

  python3 -m venv /tmp/fontenv && /tmp/fontenv/bin/pip install fonttools brotli
  /tmp/fontenv/bin/python tools/pumpernickel.py

The glyphs below are its source: a string per glyph, its rows top to bottom
separated by spaces, '#' set and '.' clear; they start at row 1 unless the
string begins with the row they start on.
"""
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

OUT = Path(__file__).resolve().parent.parent / 'docs/play/themes/fonts/pumpernickel.woff2'
PX = 100
ROWS = 12
BASE = 10  # rows above the baseline (0-9)

G = {}


def g(chars, art):
    """Glyph(s) `chars` from `art`: rows of '#' and '.', from row 1, or from the row a leading number says."""
    lines = art.split()
    start = 1
    if lines[0].isdigit():
        start = int(lines.pop(0))
    width = max(len(ln) for ln in lines)
    rows = ['.' * width] * ROWS
    for i, ln in enumerate(lines):
        rows[start + i] = ln.ljust(width, '.')
    for ch in chars:
        G[ch] = rows


# ── capitals: rows 1-9 ──────────────────────────────────────────────────────
g('A', '..##.. .####. ##..## ##..## ##..## ###### ##..## ##..## ##..##')
g('B', '#####. ##..## ##..## ##..## #####. ##..## ##..## ##..## #####.')
g('C', '.####. ##..## ##.... ##.... ##.... ##.... ##.... ##..## .####.')
g('D', '#####. ##..## ##..## ##..## ##..## ##..## ##..## ##..## #####.')
g('E', '###### ##.... ##.... ##.... #####. ##.... ##.... ##.... ######')
g('F', '###### ##.... ##.... ##.... #####. ##.... ##.... ##.... ##....')
g('G', '.####. ##..## ##.... ##.... ##.### ##..## ##..## ##..## .#####')
g('H', '##..## ##..## ##..## ##..## ###### ##..## ##..## ##..## ##..##')
g('I', '## ## ## ## ## ## ## ## ##')
g('J', '....## ....## ....## ....## ....## ....## ##..## ##..## .####.')
g('K', '##...## ##..##. ##.##.. ####... ###.... ####... ##.##.. ##..##. ##...##')
g('L', '##.... ##.... ##.... ##.... ##.... ##.... ##.... ##.... ######')
g('M', '##...## ###.### ####### ##.#.## ##...## ##...## ##...## ##...## ##...##')
g('N', '##..## ###.## ###.## ###### ##.### ##.### ##..## ##..## ##..##')
g('O', '.####. ##..## ##..## ##..## ##..## ##..## ##..## ##..## .####.')
g('P', '#####. ##..## ##..## ##..## #####. ##.... ##.... ##.... ##....')
g('Q', '.####. ##..## ##..## ##..## ##..## ##..## ##..## ##.### .####. ....##')
g('R', '#####. ##..## ##..## ##..## #####. ####.. ##.##. ##..## ##..##')
g('S', '.####. ##..## ##.... ###... .####. ...### ....## ##..## .####.')
g('T', '###### ..##.. ..##.. ..##.. ..##.. ..##.. ..##.. ..##.. ..##..')
g('U', '##..## ##..## ##..## ##..## ##..## ##..## ##..## ##..## .####.')
g('V', '##..## ##..## ##..## ##..## ##..## ##..## .####. .####. ..##..')
g('W', '##...## ##...## ##...## ##...## ##.#.## ##.#.## ####### .##.##. .#...#.')
g('X', '##..## ##..## ##..## .####. ..##.. .####. ##..## ##..## ##..##')
g('Y', '##..## ##..## ##..## .####. ..##.. ..##.. ..##.. ..##.. ..##..')
g('Z', '###### ....## ....## ...##. ..##.. .##... ##.... ##.... ######')

# ── lower case: x-height rows 4-9, ascenders from row 1, descenders to row 11 ──
g('a', '4 .####. ....## .##### ##..## ##..## .#####')
g('b', '##.... ##.... ##.... #####. ##..## ##..## ##..## ##..## #####.')
g('c', '4 .####. ##..## ##.... ##.... ##..## .####.')
g('d', '....## ....## ....## .##### ##..## ##..## ##..## ##..## .#####')
g('e', '4 .####. ##..## ###### ##.... ##..## .####.')
g('f', '..### .##.. .##.. ####. .##.. .##.. .##.. .##.. .##..')
g('g', '4 .##### ##..## ##..## ##..## ##..## .##### ....## .####.')
g('h', '##.... ##.... ##.... #####. ##..## ##..## ##..## ##..## ##..##')
g('i', '2 ## .. ## ## ## ## ## ##')
g('j', '2 ..## .... ..## ..## ..## ..## ..## ..## ..## ###.')
g('k', '##.... ##.... ##.... ##..## ##.##. ####.. ####.. ##.##. ##..##')
g('l', '## ## ## ## ## ## ## ## ##')
g('m', '4 #######. ##.##.## ##.##.## ##.##.## ##.##.## ##.##.##')
g('n', '4 #####. ##..## ##..## ##..## ##..## ##..##')
g('o', '4 .####. ##..## ##..## ##..## ##..## .####.')
g('p', '4 #####. ##..## ##..## ##..## ##..## #####. ##.... ##....')
g('q', '4 .##### ##..## ##..## ##..## ##..## .##### ....## ....##')
g('r', '4 ##.### ####.. ##.... ##.... ##.... ##....')
g('s', '4 .##### ##.... .####. ....## ....## #####.')
g('t', '2 .##. .##. #### .##. .##. .##. .##. ..##')
g('u', '4 ##..## ##..## ##..## ##..## ##..## .#####')
g('v', '4 ##..## ##..## ##..## .####. .####. ..##..')
g('w', '4 ##...## ##...## ##.#.## ##.#.## ####### .##.##.')
g('x', '4 ##..## .####. ..##.. ..##.. .####. ##..##')
g('y', '4 ##..## ##..## ##..## ##..## ##..## .##### ....## .####.')
g('z', '4 ###### ...##. ..##.. .##... ##.... ######')

# ── figures: rows 1-9 ───────────────────────────────────────────────────────
g('0', '.####. ##..## ##..## ##.### ###### ###.## ##..## ##..## .####.')
g('1', '..## .### #### ..## ..## ..## ..## ..## ..##')
g('2', '.####. ##..## ....## ....## ...##. ..##.. .##... ##.... ######')
g('3', '.####. ##..## ....## ....## ..###. ....## ....## ##..## .####.')
g('4', '...##. ..###. .####. ##.##. ##.##. ###### ...##. ...##. ...##.')
g('5', '###### ##.... ##.... #####. ....## ....## ....## ##..## .####.')
g('6', '..###. .##... ##.... #####. ##..## ##..## ##..## ##..## .####.')
g('7', '###### ....## ....## ...##. ...##. ..##.. ..##.. ..##.. ..##..')
g('8', '.####. ##..## ##..## ##..## .####. ##..## ##..## ##..## .####.')
g('9', '.####. ##..## ##..## ##..## .##### ....## ....## ...##. .###..')

# ── punctuation and signs ───────────────────────────────────────────────────
g('!', '## ## ## ## ## ## .. ## ##')
g('"', '##.## ##.## ##.##')
g("'‘’", '## ## ##')
g('“”', '##.## ##.## ##.##')
g('#', '2 .##.##. .##.##. ####### .##.##. ####### .##.##. .##.##.')
g('$', '..##.. .####. ##.... ##.... .####. ....## ....## .####. ..##..')
g('%', '##....# ##...## ....##. ...##.. ..##... .##.... ##...## #....##')
g('&', '.###.. ##.##. ##.##. .###.. ###.## ##.##. ##..## ##.### .###.#')
g('(', '..## .##. ##.. ##.. ##.. ##.. ##.. .##. ..##')
g(')', '##.. .##. ..## ..## ..## ..## ..## .##. ##..')
g('*', '3 ##..## .####. ###### .####. ##..##')
g('+', '3 ..##.. ..##.. ###### ..##.. ..##..')
g(',', '8 ## ## .# #.')
g('-', '5 #####')
g('–', '5 ######')
g('—', '5 ##########')
g('.', '8 ## ##')
g('…', '8 ##.##.## ##.##.##')
g('·', '5 ## ##')
g('/', '....## ....## ...##. ...##. ..##.. .##... .##... ##.... ##....')
g('\\', '##.... ##.... .##... .##... ..##.. ...##. ...##. ....## ....##')
g(':', '4 ## ## .. .. ## ##')
g(';', '4 ## ## .. .. ## ## .# #.')
g('<‹', '2 ...## ..##. .##.. ##... .##.. ..##. ...##')
g('>›', '2 ##... .##.. ..##. ...## ..##. .##.. ##...')
g('=', '4 ##### ..... #####')
g('?', '.####. ##..## ....## ...##. ..##.. ..##.. ...... ..##.. ..##..')
g('@', '.#####. ##...## ##.#### ##.##.# ##.##.# ##.#### ##..... ##...## .#####.')
g('[', '#### ##.. ##.. ##.. ##.. ##.. ##.. ##.. ####')
g(']', '#### ..## ..## ..## ..## ..## ..## ..## ####')
g('^', '..##.. .####. ##..##')
g('_', '10 ######')
g('\u0060', '##. .##')
g('{', '..### .##.. .##.. .##.. ##... .##.. .##.. .##.. ..###')
g('|', '0 ## ## ## ## ## ## ## ## ## ## ## ##')
g('}', '###.. ..##. ..##. ..##. ...## ..##. ..##. ..##. ###..')
g('~', '4 .###.## ##.###.')
g('★', '1 ...##... ...##... ..####.. ######## .######. ..####.. .##..##. .#....#.')
g('▶▸', '2 ##..... ####... ######. ####### ######. ####... ##.....')
g('✕×', '3 ##..## .####. ..##.. .####. ##..##')
g('☰≡', '3 ###### ...... ###### ...... ######')
g('»', '3 ##.##.. .##.##. ..##.## .##.##. ##.##..')
g('«', '3 ..##.## .##.##. ##.##.. .##.##. ..##.##')
g('■', '3 ##### ##### ##### ##### #####')


# ── accented letters: the base glyph with its mark above (rows 1-2 over lower case, row 0 over a capital),
#    or the cedilla under it; each mark drawn for the six-pixel letters it sits on ──────────────────────
MARKS = {
    'diaeresis': ['##..##'], 'acute': ['...##.', '..##..'], 'grave': ['.##...', '..##..'],
    'circumflex': ['..##..', '.#..#.'], 'tilde': ['.##..#', '#..##.'], 'ring': ['.####.', '.#..#.'],
}
ACCENTS = {
    'ä': 'a diaeresis', 'ë': 'e diaeresis', 'ö': 'o diaeresis', 'ü': 'u diaeresis', 'ÿ': 'y diaeresis',
    'á': 'a acute', 'é': 'e acute', 'ó': 'o acute', 'ú': 'u acute', 'à': 'a grave', 'è': 'e grave', 'ò': 'o grave',
    'ù': 'u grave', 'â': 'a circumflex', 'ê': 'e circumflex', 'ô': 'o circumflex', 'û': 'u circumflex',
    'ñ': 'n tilde', 'ã': 'a tilde', 'õ': 'o tilde', 'å': 'a ring',
    'Ä': 'A diaeresis', 'Ö': 'O diaeresis', 'Ü': 'U diaeresis', 'É': 'E acute', 'È': 'E grave', 'Á': 'A acute',
    'À': 'A grave', 'Ñ': 'N tilde', 'Å': 'A ring',
}


def compose():
    for ch, how in ACCENTS.items():
        base, mark = how.split()
        rows = list(G[base])
        lines = MARKS[mark] if base.islower() else MARKS[mark][-1:]  # (a capital has room for one row)
        top = 3 - len(lines) if base.islower() else 0
        for i, line in enumerate(lines):
            rows[top + i] = line.ljust(len(rows[0]), '.')
        G[ch] = rows
    for ch, base in {'ç': 'c', 'Ç': 'C'}.items():  # the cedilla, hung under the letter's middle
        rows = list(G[base])
        rows[10], rows[11] = '..##..', '.##...'
        G[ch] = rows


compose()


def outline(rows):
    """The pixels as outlines: one rectangle per run of set pixels in a row."""
    pen = TTGlyphPen(None)
    for y, row in enumerate(rows):
        top = (BASE - y) * PX
        x = 0
        while x < len(row):
            if row[x] != '#':
                x += 1
                continue
            end = x
            while end < len(row) and row[end] == '#':
                end += 1
            pen.moveTo((x * PX, top - PX))
            pen.lineTo((x * PX, top))
            pen.lineTo((end * PX, top))
            pen.lineTo((end * PX, top - PX))
            pen.closePath()
            x = end
    return pen.glyph()


def main():
    shapes = dict(G)
    names = ['.notdef', 'space'] + [f'u{ord(ch):04X}' for ch in shapes]
    box = ['......'] + ['######'] + ['#....#'] * 7 + ['######'] + ['......'] * 2
    glyphs = {'.notdef': outline(box), 'space': outline(['.'] * ROWS)}
    metrics = {'.notdef': (7 * PX, 0), 'space': (4 * PX, 0)}  # a word space of four pixels
    for ch, rows in shapes.items():
        name = f'u{ord(ch):04X}'
        glyphs[name] = outline(rows)
        metrics[name] = ((len(rows[0]) + 1) * PX, 0)  # as wide as its pixels, and one of space
    fb = FontBuilder(ROWS * PX, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({32: 'space', 160: 'space', **{ord(ch): f'u{ord(ch):04X}' for ch in shapes}})
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=BASE * PX, descent=-(ROWS - BASE) * PX)
    fb.setupNameTable({
        'familyName': 'Pumpernickel', 'styleName': 'Regular',
        'copyright': 'Pumpernickel: drawn for breadbin (tools/pumpernickel.py)',
    })
    fb.setupOS2(sTypoAscender=BASE * PX, sTypoDescender=-(ROWS - BASE) * PX, sTypoLineGap=0,
                usWinAscent=BASE * PX, usWinDescent=(ROWS - BASE) * PX, sxHeight=6 * PX, sCapHeight=9 * PX, fsType=0)
    fb.setupPost()
    fb.font.flavor = 'woff2'
    OUT.parent.mkdir(parents=True, exist_ok=True)
    fb.save(OUT)
    print(f'{OUT}: Pumpernickel, {len(shapes)} characters, {OUT.stat().st_size} bytes')


if __name__ == '__main__':
    main()
