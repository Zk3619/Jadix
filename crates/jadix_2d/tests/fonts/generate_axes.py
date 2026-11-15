"""Original variable font with gvar/HVAR and liga for instance-consistency tests."""
from pathlib import Path
from tempfile import TemporaryDirectory
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.designspaceLib import DesignSpaceDocument, AxisDescriptor, SourceDescriptor
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.varLib import build

folder=Path(__file__).parent
with TemporaryDirectory() as tmp:
 ds=DesignSpaceDocument()
 for name,tag,lo,default,hi in [('Width','wdth',50,100,200),('Weight','wght',100,400,900)]:
  a=AxisDescriptor();a.name=name;a.tag=tag;a.minimum=lo;a.default=default;a.maximum=hi;ds.addAxis(a)
 for i,(width,weight) in enumerate([(100,400),(50,400),(200,400),(100,100),(100,900)]):
  fb=FontBuilder(1000,isTTF=True);names=['.notdef','space','A','f','i','fi']
  fb.setupGlyphOrder(names);fb.setupCharacterMap({32:'space',65:'A',102:'f',105:'i'})
  ink=int(4*width);metrics={};glyphs={}
  for n in names:
   pen=TTGlyphPen(None)
   if n not in ['.notdef','space']:
    h=600+int(weight/5);w=ink if n!='fi' else ink*2
    pen.moveTo((0,0));pen.lineTo((w,0));pen.lineTo((w,h));pen.lineTo((0,h));pen.closePath()
   glyphs[n]=pen.glyph();metrics[n]=(int(5*width)*(2 if n=='fi' else 1),0)
  fb.setupGlyf(glyphs);fb.setupHorizontalMetrics(metrics)
  fb.setupHorizontalHeader(ascent=1000,descent=-200)
  fb.setupNameTable(dict(familyName='Jadix Axes Fixture',styleName='Regular',uniqueFontIdentifier='JadixAxes'+str(i),fullName='Jadix Axes Fixture',psName='JadixAxes'+str(i)))
  fb.setupOS2(sTypoAscender=1000,sTypoDescender=-200,usWinAscent=1000,usWinDescent=200)
  fb.setupPost();fb.setupMaxp();fb.font['head'].created=fb.font['head'].modified=3800000000
  addOpenTypeFeaturesFromString(fb.font,'languagesystem DFLT dflt; feature liga { sub f i by fi; } liga;')
  path=Path(tmp)/('master'+str(i)+'.ttf');fb.save(path)
  source=SourceDescriptor();source.path=str(path);source.name='master'+str(i);source.familyName='Jadix Axes Fixture';source.styleName='Regular';source.location={'Width':width,'Weight':weight}
  if i==0:source.copyInfo=True;source.copyLib=True;source.copyFeatures=True
  ds.addSource(source)
 font,_,_=build(ds);font['head'].created=font['head'].modified=3800000000;font.recalcTimestamp=False;font.save(folder/'axes-mini.ttf')
