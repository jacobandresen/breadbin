#!/usr/bin/env python3
"""Match Lemon64 popularity scores to the local C64 collection.

Reads  c64_popularity.tsv  (rank <tab> score <tab> votes <tab> title), walks the
game library, and writes  c64_index.tsv  containing ONLY games that exist both in
the collection and on the Lemon64 ranked list, ordered by popularity rank.

Matching is equality-only on the cleaned leading title (no substring/prefix
fallback, which would mis-rank e.g. Bruce Lee vs Bruce Lee II). Roman<->arabic
numerals are normalised on both sides as the one safe recall lever.

Output line:  display<TAB>path
"""
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT  = os.path.join(HERE, "c64_index.tsv")
IA_INDEX = os.path.join(HERE, "ia_index.tsv")          # IA games with cover + details

C64_LIB = os.path.expanduser(os.environ.get("C64_LIB", "~/Games/Commodore/C64"))
LIB_DIRS = [
    C64_LIB,
]
# breadbin's own download folders — games fetched here also show in the menu (as extras),
# even when they're not in the ranked list. NOT the whole library (that would flood it).
DOWNLOAD_DIRS = [
    os.path.join(C64_LIB, "_IA_downloads"),
    os.path.join(C64_LIB, "_UTA_downloads"),
]
EXTS = (".d64", ".d71", ".d81", ".t64", ".tap", ".crt", ".g64", ".nib", ".p00", ".x64")

ROMAN = {"i": "1", "ii": "2", "iii": "3", "iv": "4", "v": "5", "vi": "6",
         "vii": "7", "viii": "8", "ix": "9", "x": "10"}

# deterministic synonym expansion so alternative title forms canonicalise the same.
# numerals are preserved (II stays distinct from III), so this is safe for equality.
ABBREV = {"jr": "junior", "bros": "brothers", "intl": "international",
          "vs": "versus", "n": "and"}

def norm(s: str) -> str:
    s = s.lower()
    s = s.replace("&", " and ")
    s = re.sub(r"\b(jr|bros|intl|vs|n)\b\.?",
               lambda m: ABBREV[m.group(1)], s)          # Jr.->junior, 'n->and, &->and
    s = re.sub(r"\b(i{1,3}|iv|v|vi{0,3}|ix|x)\b", lambda m: ROMAN[m.group(1)], s)
    s = re.sub(r"\b(the|a)\b", " ", s)
    s = re.sub(r"[^a-z0-9]", "", s)
    return s

def title_key(name: str) -> str:
    """Cleaned leading-title key for a popularity title."""
    name = re.split(r":| - ", name, maxsplit=1)[0]   # drop subtitle
    return norm(name)

def nice_title(basename: str) -> str:
    """A readable game name from a ROM filename (for downloaded extras)."""
    stem = re.sub(r"\.[a-z0-9]{2,4}$", "", basename, flags=re.I).replace("_", " ")
    stem = re.split(r"[\(\[]", stem, maxsplit=1)[0]
    m = re.search(r"\S\s(?:19xx|19\d\d|20\d\d)(?:\s|$)", stem)    # paren-less year+publisher
    if m: stem = stem[:m.start() + 1]
    stem = re.split(r" - | (?:Side|Tape|Disk|Part) \d", stem, maxsplit=1)[0]
    stem = re.sub(r"\bv\d+(\.\d+)*\b", "", stem)
    return re.sub(r"\s+", " ", stem).strip()

def file_key(basename: str) -> str:
    """Cleaned leading-title key for a ROM filename."""
    stem = re.sub(r"\.[a-z0-9]{2,4}$", "", basename, flags=re.I)
    stem = stem.replace("_", " ")                        # UTA uses underscores
    stem = re.split(r"[\(\[]", stem, maxsplit=1)[0]      # drop (year)(publisher)[flags]
    # IA names embed year+publisher with no parens (e.g. "Wasteland 1988 Electronic Arts ...");
    # cut at the first space-delimited year that has title text before it (keeps games like "1942").
    m = re.search(r"\S\s(?:19xx|19\d\d|20\d\d)(?:\s|$)", stem)
    if m:
        stem = stem[:m.start() + 1]
    stem = re.split(r" - | (?:Side|Tape|Disk|Part) \d", stem, maxsplit=1)[0]  # drop subtitle / media descriptor
    stem = re.sub(r"\bv\d+(\.\d+)*\b", "", stem)         # drop version (v1.0)
    return norm(stem)

def main():
    # collect files grouped by their cleaned title key
    by_key = {}
    for d in LIB_DIRS:
        for root, _, files in os.walk(d):
            for f in files:
                if f.lower().endswith(EXTS):
                    by_key.setdefault(file_key(f), []).append(os.path.join(root, f))

    def load_rank(b):
        # multi-load media must boot from the lowest tape/disk then side; (1,0)=single file
        b = b.replace("_", " ")                          # UTA uses underscores
        def num(kind):
            m = re.search(rf"\b{kind}\s*([1-9])\b", b, re.I)
            if m: return int(m.group(1))
            m = re.search(rf"\b{kind}\s*([a-i])\b", b, re.I)   # Side A/B/C -> 1/2/3
            if m: return ord(m.group(1).lower()) - ord("a") + 1
            return None
        tape = num("tape") or num("disk") or num("part") or 1
        side = num("side") or (1 if re.search(r"\bstart\b", b, re.I) else
                               3 if re.search(r"\bend\b", b, re.I) else 0)
        return (tape, side)

    def pick(paths):
        # lowest tape/side, then fewest [..] flags, then shortest name
        return min(paths, key=lambda p: (load_rank(os.path.basename(p)),
                                         os.path.basename(p).count("["),
                                         len(os.path.basename(p))))

    if not os.path.exists(IA_INDEX):
        print("no ia_index.tsv yet (run: c64menu --refresh); keeping existing index",
              file=sys.stderr)
        return                                             # don't wipe a shipped index

    # Each game is on IA with GameBase64 details, already ranked by popularity (GB64
    # rating, then IA downloads). Local copies are matched by canonical title; the
    # rest are downloadable from IA. Order is preserved.
    # columns: display <TAB> status <TAB> target <TAB> title <TAB> query <TAB> identifier <TAB> genre
    n_local = n_avail = 0
    with open(OUT, "w", encoding="utf-8") as out:
        for line in open(IA_INDEX, encoding="utf-8"):
            canon, rating, downloads, genre, ident, title = line.rstrip("\n").split("\t", 5)
            paths = by_key.get(canon)
            if paths:
                status, target = "local", pick(paths); n_local += 1
            else:
                status, target = "available", "ia"; n_avail += 1
            disp = title + (f"   ★{rating}" if rating not in ("0", "") else "")
            out.write(f"{disp}\t{status}\t{target}\t{title}\t{title}\t{ident}\t{genre}\n")

    print(f"{n_local} local + {n_avail} downloadable = {n_local + n_avail} "
          f"games (IA · details, ranked) -> {OUT}", file=sys.stderr)

if __name__ == "__main__":
    main()
