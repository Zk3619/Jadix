"""Original static SVG/SVGZ font fixtures (MIT/Apache with the repository)."""
from pathlib import Path
from fontTools.ttLib import TTFont,newTable
from fontTools.ttLib.tables.S_V_G_ import SVGDocument
from PIL import Image
import base64,gzip,io

folder=Path(__file__).parent
font=TTFont(folder/'colr-v1.ttf')
del font['COLR'];del font['fvar']
for record in font['name'].names:
    if record.nameID in [1,3,4,6]:record.string='Jadix SVG Fixture'.encode(record.getEncoding())
font['SVG ']=newTable('SVG ')
font['SVG '].docList=[]
buffer=io.BytesIO()
image=Image.new('RGBA',(2,1));image.putdata([(255,0,0,128),(0,0,255,255)]);image.save(buffer,format='PNG')
png=base64.b64encode(buffer.getvalue()).decode()
for i in range(11):
    gid=4+i
    contents=f'<rect x="0" y="-800" width="400" height="800" fill="var(--color0, red)"/><rect x="400" y="-800" width="400" height="800" fill="currentColor"/>'
    if i==2:contents=f'<image x="0" y="-800" width="800" height="800" preserveAspectRatio="none" image-rendering="optimizeSpeed" href="data:image/png;base64,{png}"/>'
    if i==3:contents='<image x="0" y="-800" width="800" height="800" href="file:///never-read-this.png"/>'
    if i==4:contents='<text x="0" y="0">unsupported</text>'
    node=f'wrong{gid}' if i==5 else f'glyph{gid}'
    if i==6:contents+=' '*100000
    xml=f'<svg xmlns="http://www.w3.org/2000/svg"><g id="{node}">{contents}</g></svg>'
    if i==7:xml=f'<svg xmlns="http://www.w3.org/2000/svg" id="{node}" viewBox="0 1000 1000 1000"><g transform="translate(0,1000)">{contents}</g></svg>'
    if i==8:xml=f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 1000 1000 1000" data-test="a > b"><defs><g id="{node}" transform="translate(0,1000)">{contents}</g></defs><rect x="0" y="200" width="800" height="800" fill="blue"/></svg>'
    if i==9:xml=f'<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(5000,5000)" opacity="0.1"><g id="{node}">{contents}</g></g></svg>'
    if i==10:xml=f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 2000 2000 2000"><g id="{node}" transform="translate(0,2000) scale(2)">{contents}</g></svg>'
    font['SVG '].docList.append(SVGDocument(xml,gid,gid,compressed=i in [1,6]))
font['head'].created=font['head'].modified=3800000000
font.save(folder/'svg-mini.ttf')
