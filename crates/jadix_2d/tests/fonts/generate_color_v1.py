"""Original bounded COLRv1 fixture: all paints/composites, transforms and variations.

Test data is MIT/Apache licensed with this repository. FontTools is needed only
to regenerate; Rust tests consume the committed deterministic binary.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.ttLib.tables.otTables import PaintFormat as P
from fontTools.varLib.varStore import OnlineVarStoreBuilder

def solid(index=0, alpha=1):
    return dict(Format=P.PaintSolid, PaletteIndex=index, Alpha=alpha)

def ink(paint, glyph='box'):
    return dict(Format=P.PaintGlyph, Glyph=glyph, Paint=paint)

def line(extend=0):
    return dict(Extend=extend, ColorStop=[
        dict(StopOffset=0., PaletteIndex=0, Alpha=1.),
        dict(StopOffset=.5, PaletteIndex=1, Alpha=.5),
        dict(StopOffset=1., PaletteIndex=0xFFFF, Alpha=1.)])

paints = {
    'solid': ink(solid()),
    'foreground': ink(solid(0xFFFF)),
    'linear': ink(dict(Format=P.PaintLinearGradient, ColorLine=line(), x0=0,y0=0,x1=800,y1=0,x2=200,y2=800)),
    'radial': ink(dict(Format=P.PaintRadialGradient, ColorLine=line(), x0=200,y0=300,r0=0,x1=400,y1=400,r1=600)),
    'sweep': ink(dict(Format=P.PaintSweepGradient, ColorLine=line(), centerX=400,centerY=400,startAngle=0,endAngle=360)),
    'repeat': ink(dict(Format=P.PaintLinearGradient, ColorLine=line(1), x0=0,y0=0,x1=200,y1=0,x2=0,y2=800)),
    'reflect': ink(dict(Format=P.PaintSweepGradient, ColorLine=line(2),centerX=400,centerY=400,startAngle=45,endAngle=135)),
    'wide-sweep': ink(dict(Format=P.PaintSweepGradient,ColorLine=line(),centerX=400,centerY=400,startAngle=-100,endAngle=500)),
    'transform': dict(Format=P.PaintTransform, Paint=ink(solid(1),'triangle'),Transform=dict(xx=.8,yx=.2,xy=-.2,yy=.8,dx=150,dy=0)),
    'nested': dict(Format=P.PaintColrLayers,Layers=[ink(solid(0)),dict(Format=P.PaintTranslate,dx=200,dy=100,Paint=ink(solid(0xFFFF,.5),'triangle'))]),
    'reference': dict(Format=P.PaintColrGlyph,Glyph='linear'),
    'variable': dict(Format=P.PaintVarTranslate,dx=0,dy=0,VarIndexBase=0,Paint=ink(solid(1))),
    'cycle':dict(Format=P.PaintColrGlyph,Glyph='cycle'),
}
for mode in range(28):
    paints['compose'+str(mode)] = dict(Format=P.PaintComposite,
        CompositeMode=mode,BackdropPaint=ink(solid(0,.8)),
        SourcePaint=dict(Format=P.PaintTranslate,dx=200,dy=100,Paint=ink(solid(1,.7),'triangle')))

paints['unbounded'] = solid()
paints['empty'] = ink(solid(), 'space')

names=['.notdef','space','box','triangle']+list(paints)
fb=FontBuilder(1000,isTTF=True)
fb.setupGlyphOrder(names)
fb.setupCharacterMap({32:'space',**{0xE100+i:n for i,n in enumerate(paints)}})
glyphs={}
for name in names:
    pen=TTGlyphPen(None)
    if name=='box':
        pen.moveTo((0,0));pen.lineTo((800,0));pen.lineTo((800,800));pen.lineTo((0,800));pen.closePath()
    elif name=='triangle':
        pen.moveTo((0,0));pen.lineTo((600,0));pen.lineTo((300,700));pen.closePath()
    glyphs[name]=pen.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({n:(1000,0) for n in names})
fb.setupHorizontalHeader(ascent=1000,descent=-200)
fb.setupNameTable(dict(familyName='Jadix COLRv1 Fixture',styleName='Regular',
    uniqueFontIdentifier='Jadix COLRv1 Fixture 1',fullName='Jadix COLRv1 Fixture',psName='JadixCOLRv1Fixture'))
fb.setupOS2(sTypoAscender=1000,sTypoDescender=-200,usWinAscent=1000,usWinDescent=200)
fb.setupPost();fb.setupMaxp()
fb.setupFvar([('wght',100,400,900,'Weight')],[])
store=OnlineVarStoreBuilder(['wght'])
store.setSupports([{'wght':(0,1,1)}])
assert store.storeDeltas([200])==0
assert store.storeDeltas([0])==1
fb.font['COLR']=buildCOLR(paints,version=1,glyphMap=fb.font.getReverseGlyphMap(),
    varStore=store.finish(),clipBoxes={n:(0,0,1000,1000) for n in paints if n not in ['unbounded','empty']})
fb.font['CPAL']=buildCPAL([[(1.,0.,0.,.5),(0.,0.,1.,1.)],[(0.,1.,0.,1.),(1.,1.,0.,.8)]])
fb.font['head'].created=fb.font['head'].modified=3800000000
fb.save(Path(__file__).with_name('colr-v1.ttf'))

