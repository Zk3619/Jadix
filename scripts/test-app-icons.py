"""Asset output contracts; run: python scripts/test-app-icons.py."""
import importlib.util
import json
from pathlib import Path
import plistlib
import tempfile
import unittest
import xml.etree.ElementTree as ET
from PIL import Image

spec = importlib.util.spec_from_file_location('app_icons', Path(__file__).with_name('app-icons.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class IconAssets(unittest.TestCase):
    def test_all_platform_assets_and_alternate(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            Image.new('RGBA', (1024,1024), (255,0,0,200)).save(root/'master.png')
            config = root/'icons.toml'
            config.write_text('source="master.png"\n[android]\nmonochrome="master.png"\n[ios.alternates]\nDark="master.png"\n', encoding='utf-8')
            out = root/'out'
            files = module.generate(config,out,['desktop','android','ios'])
            self.assertTrue(all((out/f).is_file() for f in files))
            with Image.open(out/'desktop/app.ico') as icon:
                self.assertIn((16,16),icon.ico.sizes())
                self.assertIn((256,256),icon.ico.sizes())
            with Image.open(out/'desktop/app.icns') as icon:
                self.assertEqual(icon.size,(1024,1024))
            with Image.open(out/'android/res/drawable-mdpi/ic_launcher_foreground.png') as icon:
                self.assertEqual(icon.size,(108,108))
                self.assertEqual(icon.getbbox(),(21,21,87,87))
            self.assertEqual(len(ET.parse(out/'android/res/mipmap-anydpi-v26/ic_launcher.xml').getroot()),2)
            self.assertEqual(len(ET.parse(out/'android/res/mipmap-anydpi-v33/ic_launcher.xml').getroot()),3)
            contents = json.loads((out/'ios/Assets.xcassets/Dark.appiconset/Contents.json').read_text())
            for entry in contents['images']:
                with Image.open(out/'ios/Assets.xcassets/Dark.appiconset'/entry['filename']) as icon:
                    self.assertEqual(icon.mode,'RGB')
                    self.assertEqual(icon.width,round(float(entry['size'].split('x')[0])*int(entry['scale'][0])))
            plist = plistlib.loads((out/'ios/Info-icons.plist').read_bytes())
            self.assertEqual(plist['CFBundleIcons']['CFBundleAlternateIcons']['Dark']['CFBundleIconName'],'Dark')

    def test_invalid_source_and_transparent_background(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            config = root/'icons.toml'
            config.write_text('source="master.png"',encoding='utf-8')
            Image.new('RGBA',(10,20)).save(root/'master.png')
            with self.assertRaisesRegex(ValueError,'square'):
                module.generate(config,root/'out',['desktop'])
            Image.new('RGBA',(20,20)).save(root/'master.png')
            config.write_text('source="master.png"\n[android]\nbackground="#ffffff00"',encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'opaque'):
                module.generate(config,root/'out',['android'])

if __name__ == '__main__':
    unittest.main()
