"""Enumerate reachable public APIs from pinned rustdoc JSON, without regex guesses.

Standard-library/blanket trait methods are recorded as trait implementations on
their owner, not recursively expanded into millions of unrelated std APIs.
"""
import hashlib,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent

def scan(path):
    data=json.loads(path.read_text(encoding='utf-8'))
    index=data['index'];rows={};externals=[]
    def walk(identity,name,enum_child=False,depth=0):
        if depth>24:raise ValueError('unexpected API cycle '+name)
        item=index.get(str(identity))
        if not item:return
        kind,body=next(iter(item['inner'].items()))
        if kind=='impl':return
        row={'path':name,'kind':kind,'id':identity,'crate_id':item['crate_id'],
             'span':item.get('span'),'docs':item.get('docs') or '',
             'attrs':item.get('attrs',[]),'deprecation':item.get('deprecation'),
             'signature':body}
        # Impl contents are inventoried below, so retain only their IDs here.
        key=(name,kind)
        if key in rows:return
        rows[key]=row
        if kind=='use':
            alias=body['name'];target=index.get(str(body['id']))
            row['reexport_source']=body['source']
            if target and target['crate_id']==0:walk(body['id'],name,depth=depth+1)
            else:externals.append({'path':name,'source':body['source'],'target_id':body['id']})
        if kind=='module':
            for child in body['items']:
                value=index.get(str(child))
                if not value or value['visibility']!='public':continue
                export=value['inner'].get('use',{})
                if export.get('is_glob'):
                    target=index.get(str(export['id']),{})
                    module=target.get('inner',{}).get('module')
                    if module and target.get('crate_id')==0:
                        for imported in module['items']:
                            obj=index.get(str(imported))
                            if obj and obj['visibility']=='public':
                                imported_name=obj['inner'].get('use',{}).get('name',obj['name'])
                                if imported_name:walk(imported,name+'::'+imported_name,depth=depth+1)
                        continue
                child_name=value['inner'].get('use',{}).get('name',value['name'])
                if child_name:walk(child,name+'::'+child_name,depth=depth+1)
        fields=[]
        if kind=='struct':
            shape=body['kind']
            if isinstance(shape,dict):fields=shape.get('plain',{}).get('fields',shape.get('tuple',[]))
        elif kind=='union':fields=body['fields']
        elif kind=='enum':
            for child in body['variants']:
                v=index[str(child)];walk(child,name+'::'+v['name'],True,depth+1)
        elif kind=='variant':
            shape=body['kind']
            if isinstance(shape,dict):fields=shape.get('struct',{}).get('fields',shape.get('tuple',[]))
        for child in fields:
            if child is None:continue
            v=index[str(child)]
            if enum_child or v['visibility']=='public':walk(child,name+'::'+str(v['name']),depth=depth+1)
        if kind in ['struct','enum','union']:
            traits=[]
            for impl_id in body.get('impls',[]):
                impl=index[str(impl_id)]['inner']['impl']
                if impl.get('trait') is not None:
                    if impl.get('blanket_impl') is None:traits.append(impl['trait']['path'])
                    continue
                for child in impl['items']:
                    v=index[str(child)]
                    if v['visibility']=='public':walk(child,name+'::'+v['name'],depth=depth+1)
            row['trait_impls']=sorted(set(traits))
        if kind=='trait':
            for child in body['items']:
                v=index[str(child)];walk(child,name+'::'+v['name'],depth=depth+1)
    walk(data['root'],index[str(data['root'])]['name'])
    return {'crate_version':data['crate_version'],'format_version':data['format_version'],
            'target':data.get('target'),'includes_private':data['includes_private'],
            'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
            'external_reexports':externals,'items':sorted(rows.values(),key=lambda r:(r['path'],r['kind']))}

if __name__=='__main__':
    destination=HERE/'inventory';destination.mkdir(exist_ok=True)
    inputs=list(HERE.glob('*.json'))
    inputs+=[Path(sys.argv[1]) if len(sys.argv)>1 else ROOT/'crates/jadix_2d/evidence/round24/jadix_2d.rustdoc.json']
    for path in inputs:
        if not path.exists() or '-source' in path.stem:continue
        if 'index' not in json.loads(path.read_text(encoding='utf-8')):continue
        result=scan(path)
        (destination/(path.stem+'.json')).write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        print(path.name,len(result['items']),flush=True)
