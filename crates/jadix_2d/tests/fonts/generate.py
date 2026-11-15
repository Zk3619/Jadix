"""Generate the original COLRv0/CPAL fixture (fonttools required only to regenerate).

Two disjoint rectangles: half-alpha palette red, and foreground color. The tiny
font is original test data, released under this repository's MIT/Apache license.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL

fb = FontBuilder(1000, isTTF=True)
names = ['.notdef', 'space', 'color', 'red', 'foreground']
fb.setupGlyphOrder(names)
fb.setupCharacterMap({32: 'space', 0xE000: 'color'})
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    if name in ['red', 'foreground']:
        x = 0 if name == 'red' else 550
        pen.moveTo((x, 0))
        pen.lineTo((x + 450, 0))
        pen.lineTo((x + 450, 800))
        pen.lineTo((x, 800))
        pen.closePath()
    glyphs[name] = pen.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({name: (1000, 550 if name == 'foreground' else 0) for name in names})
fb.setupHorizontalHeader(ascent=900, descent=-100)
fb.setupNameTable({'familyName': 'Jadix COLR Fixture', 'styleName': 'Regular',
    'uniqueFontIdentifier': 'Jadix COLR Fixture 1', 'fullName': 'Jadix COLR Fixture',
    'psName': 'JadixCOLRFixture'})
fb.setupOS2(sTypoAscender=900, sTypoDescender=-100, usWinAscent=900, usWinDescent=100)
fb.setupPost()
fb.setupMaxp()
fb.font['COLR'] = buildCOLR({'color': [('red', 0), ('foreground', 0xFFFF)]},
    version=0, glyphMap=fb.font.getReverseGlyphMap())
fb.font['CPAL'] = buildCPAL([[(1., 0., 0., 0.5)]])
fb.font['head'].created = fb.font['head'].modified = 3800000000
fb.save(Path(__file__).with_name('colr-mini.ttf'))

# An intentionally unsupported paint graph must fail explicitly, not silently
# drop color or reinterpret its base glyph as monochrome.
from fontTools.ttLib.tables.otTables import PaintFormat
fb.font['COLR'] = buildCOLR({'color': {
    'Format': PaintFormat.PaintGlyph, 'Glyph': 'red', 'Paint': {
        'Format': PaintFormat.PaintSolid, 'PaletteIndex': 0, 'Alpha': 1.,
    },
}}, version=1, glyphMap=fb.font.getReverseGlyphMap())
fb.save(Path(__file__).with_name('colr-v1-unsupported.ttf'))
