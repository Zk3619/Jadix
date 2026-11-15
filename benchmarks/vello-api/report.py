"""Produce the complete enumerated API checklist and finite comparison report."""
import collections,json
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
OUT=ROOT/'docs/vello-jadix-api-comparison.md'
EVIDENCE=ROOT/'crates/jadix_2d/evidence/round20'
CURRENT=ROOT/'crates/jadix_2d/evidence/round24'
HISTORICAL=ROOT/'crates/jadix_2d/evidence/round17'
mapping=json.loads((HERE/'mapping.json').read_text(encoding='utf-8'))
def load(name):return json.loads((HERE/'inventory'/name).read_text(encoding='utf-8'))
vello=load('vello-0.11.0.json')
jadix=load('jadix_2d.rustdoc.json')
def classify(row):
    path=row['path'];short=path.removeprefix('vello::')
    if short in mapping:return mapping[short]
    if path.startswith('vello::low_level'):
        raise ValueError('Unclassified low-level API: '+short)
    if path.startswith('vello::util'):
        return ['partial','jadix_renderer; jadix host; wgpu','Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature.']
    if path.startswith('vello::DrawGlyphs'):
        return ['partial','Font2D; GlyphRun; PreparedText2D','Prepared glyph API instead of borrowed builder. See individual methods for axes/hint/embolden differences.']
    if path.startswith('vello::Scene'):
        return ['equivalent','Scene2D; GpuCanvas','Immutable scene recording, shared subscene replay and prepared mesh snapshot. API shape differs.']
    if path.startswith('vello::Aa') or short=='RenderParams::antialiasing_method' or short=='RendererOptions::antialiasing_support':
        return ['partial','gpu::AaQuality; PathGpu::try_new','Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode.']
    if path.startswith('vello::Error'):
        return ['different','SceneError; FontError; GradientError; AtlasError; wgpu::Error','Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type.']
    if short.startswith('RenderParams::'):
        if short.endswith(('width','height')):return ['equivalent','CanvasSize; PathGpu::render(size)','Explicit render size. Target ownership/format is separate.']
        if short.endswith('base_color'):return ['equivalent','Canvas2D::set_fill_brush; fill_rect','Jadix clears transparent; encode a first full-target fill for base color.']
    if short.startswith('RendererOptions::'):
        raise ValueError('Unclassified RendererOptions field: '+short)
    if short=='RendererOptions':return ['partial','PathGpu::try_new_with_format; set_aa_quality','Format/sample/quality selection exists; Vello-specific configuration fields are listed separately.']
    if short in ['Renderer','RenderParams']:return ['partial','gpu::PathGpu','Render-to-texture entry with explicit size/target, different compute vs stencil/triangle architecture.']
    if short in ['peniko','kurbo','wgpu']:
        return ['shared' if short!='peniko' else 'partial',{'peniko':'Brush2D; GradientStops; Color2D','kurbo':'jadix_2d::kurbo','wgpu':'jadix_2d::wgpu'}[short], 'See the complete dependency appendix; reexporting a library does not imply equivalent renderer behavior.']
    if short=='Glyph':return ['equivalent','PositionedGlyph','Glyph ID and baseline x/y positions; field types differ.']
    if short=='NormalizedCoord':return ['partial','FontVariations; Font2D::with_variations','GlyphRunOptions accepts exact F2Dot14 i16 coordinates, and Font2D exposes user-space normalization; own public storage type differs.']
    if short=='FontEmbolden':return ['equivalent','FontEmbolden; GlyphRunOptions::embolden','Same Kurbo contour expansion, with finite bounds; no reshaping/advance update.']
    if path=='vello':return ['different','jadix_2d; jadix','Project boundary, not an individual operation.']
    raise ValueError('Unclassified public API: '+path)

def typ(t):
    if t is None:return '()'
    if isinstance(t,str):return t
    if 'primitive' in t:return t['primitive']
    if 'generic' in t:return t['generic']
    if 'resolved_path' in t:
        v=t['resolved_path'];a=v.get('args')
        if a and 'angle_bracketed' in a:
            args=a['angle_bracketed']['args'];return v['path']+'<'+', '.join(typ(x.get('type',x.get('lifetime',x.get('const','?')))) for x in args)+'>'
        return v['path']
    if 'borrowed_ref' in t:
        v=t['borrowed_ref'];return '&'+('mut ' if v['is_mutable'] else '')+typ(v['type'])
    if 'tuple' in t:return '('+', '.join(map(typ,t['tuple']))+')'
    if 'slice' in t:return '['+typ(t['slice'])+']'
    if 'array' in t:return '['+typ(t['array']['type'])+'; '+str(t['array']['len'])+']'
    if 'impl_trait' in t:return 'impl '+' + '.join(x.get('trait_bound',{}).get('trait',{}).get('path','…') for x in t['impl_trait'])
    return json.dumps(t,ensure_ascii=False,separators=(',',':'))
def signature(row):
    body=row['signature']
    if row['kind']=='function':
        value=body['sig'];return '('+', '.join(n+': '+typ(t) for n,t in value['inputs'])+') → '+typ(value['output'])
    if row['kind']=='struct_field':return typ(body)
    if row['kind']=='use':return body['source']
    return row['kind']
def escape(text):return str(text).replace('|','\\|').replace('\n',' ')
rows=[]
for row in vello['items']:
    status,target,note=classify(row)
    rows.append({**row,'comparison_status':status,'jadix_api':target,'comparison_note':note})
counts=collections.Counter(r['comparison_status'] for r in rows)
functions=[r for r in rows if r['kind']=='function' and 'low_level' not in r['path'] and '::util' not in r['path']]
assert all(r['path'].removeprefix('vello::') in mapping for r in functions)
(HERE/'comparison.json').write_text(json.dumps({'scope':'Reachable public vello 0.11.0 API; all-features rustdoc; separate dependency closure inventory',
    'counts':dict(counts),'rows':rows},indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
lines=['# Vello 0.11 与 Jadix 2D：完整公开 API 清单与有限实测', '',
'本文固定 `vello = 0.11.0` compute 渲染器，另将既有 `vello_gpu = 0.3.0` strip 渲染器明确区分。扫描使用 docs.rs 官方 all-features rustdoc JSON；Jadix 使用当前源码、原生 GPU/compute/text/SVG/ICC/test-hooks feature 的本地 rustdoc JSON。原始 JSON、SHA-256、完整类型签名、文档、重导出和 trait implementation 记录保存在 [机器可读 inventory](../benchmarks/vello-api/inventory/)。', '',
'**完整的含义是公开 API 枚举完整，不是所有 API 已动态证明等价。** 本文列出 vello 自有可达公开 API、全部公开固有方法/字段/枚举变体和重导出；trait 自带/blanket 方法按 owner 的 trait 清单记录，不递归扩展 std；条件编译以所保存 docs.rs all-features target 为准。依赖重导出递归边界单独列出，不把 wgpu 的上千项 GPU 接口当成 Vello 独有绘制能力。', '',
f'本次枚举 Vello **{len(rows)}** 条（含重导出别名）；Jadix 枚举 **{len(jadix["items"])}** 条。机器分类：'+', '.join(f'`{k}` {v}' for k,v in sorted(counts.items()))+'。这些数字含结构字段和内部专家 API，**不能作为能力百分比**。', '',
'状态：`equivalent` 有用途对应入口，不保证逐像素；`partial` 有显著语义/路径缺口；`missing` 当前无同等实现；`different` 架构/主机接口不直接映射；`shared` 共同依赖。所有对应入口必须结合下文限制与证据判断。', '',
 '## 已增加的能力与仍缺少的能力', '',
'| 维度 | 本轮 Jadix 可调用入口 | 固定 Vello 0.11 公开入口/差异 |', '|---|---|---|',
'| 浮点与广色域 | Color2D、Image2D::from_linear_rgba32f、RasterCanvas::new_hdr、PathGpu::try_new_with_format(Rgba16Float)、ICC 双向转换 | render_to_texture 文档规定 Rgba8Unorm；没有同类公开 HDR/ICC 输出管理入口。Jadix 仍无 OS HDR 呈现。 |',
'| 组图像效果 | Gaussian/drop-shadow/颜色矩阵/transfer/convolution/morphology 链；显式 SceneRegion halo | Scene 有隔离/混合与专用 blurred rounded rectangle；没有同形任意组滤镜链 API。算法质量/边界需逐用例验证。 |',
'| 图集管理 | AtlasCache/GlyphAtlasCache 公开预算/LRU/不可变快照/安全局部上传 | Vello 内部资源图集和 GPU texture registration/override；Jadix 本轮新增 GPU-only image registration，支持 binding 或 copy-source 输入及明确色彩/alpha 合同。 |',
'| 场景与网格 | Scene2D 子场景、PreparedSceneGpu、GpuMesh 实际硬件实例 | Scene::append 实例编码与内部缓存是不同架构；不把显式 GpuMesh 视作所有 Scene 自动优化。 |',
'| 几何布尔与高质量图像 | PathGeometry2D 的 Union/Intersection/Difference/Xor，先几何交集再 AA；Bicubic/Lanczos3 真实 CPU/GPU 采样，crop 逐 tap 限域 | Vello Scene 无几何布尔结果 API；Peniko High 提示及固定 Vello fine.wgsl 已有 Mitchell Bicubic，Jadix 本轮补齐并增加显式 Lanczos3。round19 另加入 Trilinear/Anisotropic mip 缩小采样及 SampledScene2D 整场景样本合成，round20 增加有界 Gaussian EWA 与独立区域 GPU-only mip 生成；仍不是解析全场景积分。 |',
'| 文字与 AA | shaping/layout/variable axes/COLR/bitmap/SVG 接入；采样 AA | Jadix 本轮新增低层 outline hint/embolden/归一化坐标/逐字形变换；新增 GPU/CPU Area 与显式曲线精度；与 Vello winding-integral 的边界和重复轮廓差异见独立数值参考。 |', '',
'## 绘制和资源接口逐项比对', '',
'| Vello API | 签名 | 状态 | Jadix 对应 | 实际差异 |','|---|---|---|---|---|']
for r in functions:lines.append('| '+ ' | '.join([f'`{r["path"]}`',f'`{escape(signature(r))}`',r['comparison_status'],f'`{escape(r["jadix_api"])}`',escape(r['comparison_note'])])+' |')
lines+=['','## 全部 Vello 自有公开项（含字段、变体、专家和主机 API）','','| 公开项 | 类别/源位置 | 状态 | Jadix 对应与说明 |','|---|---|---|---|']
for r in rows:
    span=r.get('span');location=(span['filename']+':'+str(span['begin'][0])) if span else 'reexport'
    lines.append(f'| `{r["path"]}` | {r["kind"]}; `{location}` | {r["comparison_status"]} | `{escape(r["jadix_api"])}` — {escape(r["comparison_note"])} |')
lines+=['','## 依赖重导出的完整逐项附录','',
'Vello 通过 Peniko 重导出 Color、Kurbo、资源句柄；wgpu 自身再重导出 wgpu-types。这些接口的完整列表分别在下表和同目录 JSON。Jadix 直接重导出同版本 Color 0.3.3/Kurbo 0.13.1；共享类型支持不自动意味着所有 Brush/Scene 路径都支持该类型的每一种输入。wgpu 是 Jadix 工作区 fork，对等类型名并不保证与 upstream Rust 类型兼容。', '']
for name,version,target,status,note in [
    ('peniko','0.6.1','Brush2D / LayerOptions / GradientStops / Image2D','partial','Jadix 自有类型；Peniko borrowed brush、GPU 图像 override 已有功能入口；资源句柄仍没有逐类型兼容。'),
    ('color','0.3.3','jadix_2d::color_types','shared','同版本真实重导出；Color2D 提供绘制/输出桥，只有明确转换的入口进入渲染。'),
    ('kurbo','0.13.1','jadix_2d::kurbo','shared','同版本真实重导出；Canvas 仍接收自身 PathCommand，需要显式转换。'),
    ('vello_encoding','0.11.0','PathGpuOp / PreparedSceneGpu','different','Vello compute encoding/resolve ABI 不兼容，不以相似名称宣称等价。'),
    ('linebender_resource_handle','0.1.1','Image2D / Font2D shared snapshots','different','Arc 资源所有权有对应设计；不是可替换的 Blob/FontData 类型。'),
    ('wgpu','30.0.0','jadix_2d::wgpu','shared-fork','upstream vs 工作区 fork；命令、适配器、buffer/texture/surface 概念共同，host 负责资源状态与平台接入。'),
    ('wgpu_types','30.0.0','jadix_2d::wgpu','shared-fork','wgpu-types 的类型/标志/描述符接口；不纳入绘制能力统计。')]:
    data=load(name+'-'+version+'.json')
    lines+=['### '+name+' '+version,'',f'{len(data["items"])} 项。{note} 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/{name}-{version}.json`。','','| 项 | 类别 | Jadix 对应 | 状态 |','|---|---|---|---|']
    for r in data['items']:
        mapped=r['path'].replace(name,target,1) if status=='shared' else target
        lines.append(f'| `{r["path"]}` | {r["kind"]} | `{mapped}` | {status} |')
lines+=['','## 累计能力边界与标明轮次的证据','',
'- **HDR/广色域**：Color2D、float Image2D、GradientStops::new_linear、RasterCanvas::new_hdr、RGBA16F PathGpu/图层/coverage/filter scratch/输出贯通。线性 sRGB 是扩展工作空间，允许负分量和大于 1；可显式转换 P3/Rec2020 等编码。尚未提供 OS HDR swapchain、HDR metadata/PQ/HLG 端到端显示管理；非 Normal blend 与 bounded imaging 滤镜仍有 LDR clamp。',
'- **CSS/色域**：与 Vello 共享 Color 0.3.3，支持 Color 4 字面量和 none、标准空间转换；Color2D::map_to 提供 Clip 和 Oklch local-MINDE。Color2D::parse 仍为字面量 parser；新增 ColorContext2D 支持调用者提供变量/currentColor/light-dark、两色 color-mix、现代相对颜色和标量 calc/min/max/clamp，真实 Edge 33 项差异 <=1 byte。仍非完整 stylesheet cascade/tokenizer/general typed CSS math；escapes/comments/device-cmyk/system colors 等未实现，不宣称全 CSS 一致性。',
'- **ICC**：可选 icc feature，通过已有 moxcms 0.8.1 extended-range 解析/转换 RGB/RGBA，限制 profile/CLUT/TRC 输入，真实合成 P3/sRGB profiles 验证。ICC LUT/意图支持仍遵循依赖本身；不声称所有 ICC profile、CMYK、多通道或 print workflow 完整覆盖。',
'- **增量 atlas**：AtlasCache<K> 显式 page/entry 预算、LRU、无移动 free rect、清空释放；Image2D immutable 更新记录 dirty rect；仅在 predecessor 不再被 CPU snapshot 持有且 GPU 纹理可复用时局部上传。旧 PreparedText 快照不会因淘汰破坏。CPU page copy 和新快照可能仍占整页，长期持有旧快照不受当前 cache 预算约束；FontSystem/Swash 缓存没有自动 LRU。',
'- **紧致/离屏滤镜**：SceneRegion 显式 origin/size/halo；负坐标源在 padded domain 上绘制滤镜后裁切。CPU 输出 RegionImage、GPU prepare_gpu 可在同样小目标绘制；本轮 SceneRegion::render_gpu 与 PathGpu::render_to_image 可直接把结果注册为后续 Canvas 图像。新增 PathGpu::set_tight_layer_bounds，自动为普通隔离组推导保守 source/filter 域并传递 texture origin；默认关闭。mask、生成透明背景内容、convolution 保留全域；不是数学最小域，也不减少全幅 compositor 目标。显式 SceneRegion 的 halo 仍由调用者预算。',
'- **硬件实例与网格**：GpuMesh 保留几何 GPU buffer，Instance step mode，一次 draw 多实例；warm 相同实例不上传；PreparedSceneGpu 避免 replay/tessellation。此显式 API 由 host render pass 负责 clip/MSAA/顺序；不是所有 Scene2D 自动硬件实例化，也不是透明的全局 mesh LRU。',
'- **几何集合**：PathGeometry2D 解析 winding 后保留共享三角形；有四种集合运算、面积/bounds/contains/变换和一次 fill。Curve flatten/Lyon 为 f32，布尔裁切 f64，输出路径再转 f32；不是 exact curve algebra，不能将三角形 contour 用于 stroke。max_work/max_triangles 显式失败；Area GPU 另有独立预算。',
'- **图像重建/缩小**：七种 ImageSampling，Mitchell Bicubic、Lanczos3、Trilinear、16-tap Anisotropic、17×17/层 Gaussian Ewa；linear-premultiplied 与明确 Extend/crop 合同。显式 CPU mip 链，以及 GPU-only region 转换/NPOT mip 生成均已实现。raw crop 仍 base bilinear；EWA 在缺失/截断 mip 时缩小支持范围至8 texel，不能当作任意 footprint 精确积分。',
'- **平台**：Windows DX12/Vulkan/native GLES 实测；Edge WebGPU/WebGL 见 [browser-runtime.json](../crates/jadix_2d/evidence/round18/browser-runtime.json)，失败同样保留；WASM check 仅证明编译。本轮未执行 Metal/mobile 真机验收，不将 Windows/Edge 实测称为这些平台的结果。',
'- **压力**：每后端 100,000 硬件实例 ×60 帧，记录 Windows peak working set/private commit/CPU 与 GPU dedicated/shared/committed 采样。操作系统计数存在采样间隔，不等于完整 driver VRAM 峰值。', '',
'详见 [round18原始实现与验收](2d-round18.md)、[round18结构化汇总](../crates/jadix_2d/evidence/round18/summary.json)。', '',
'## Vello 与 Jadix 像素和性能实测','',
'round17 固定 Vello 0.11.0 与 Jadix，在 256²/512²、三后端、Area/Balanced/Reference 上比较八场景，共144项：三组字形、compound、gradient grid、dynamic curves、blurred rectangle、duplicate nonzero thin rectangles。hinted Jadix 在 device ppem 准备。五帧预热+20帧测量并等待 GPU。Vello 内部 glyph cache 与 Jadix 本次每帧显式 run preparation 的保留工作不同，不能把结果当相同缓存策略下的算法速度。历史四场景48项保存在 round15，字形36项保存在 round16。30k 矩形round17已按双方0.8px重跑，见独立压力表。', '']
lines += ['', '### 场景适配、失败与结论边界', '',
'固定版本 Vello fine.wgsl 在最终 textureStore 前显式 unpremultiply，因此比较使用其 straight sRGB 与 Jadix linear-premultiplied 输出的实际合同；round17脚本规范化后比较。', '',
'固定 Vello ramp encoder 未使用 Peniko interpolation_cs，gradient-grid 仍用513个预展开色标匹配线性 sRGB 插值。该适配成本、AA、缓存工作均不同，不能以计时宣称算法全面更快。原始探针保留于 round15/comparison-api-probe.json。', '',
'历史 30,000 个微矩形失败：[失败记录](../crates/jadix_2d/evidence/round15/large-scene-runs.json)。Vello DX12/FXC device lost；Jadix 超出原像素阈值。round17按双方0.8px重跑；旧场景原为双方实际1px，不与新几何混算。旧失败仍保留，不将设备失败计为速度胜出。', '',
'有限数据不能证明全 API/全平台超过 Vello。低层新字形路径使用相同 Skrifa/Kurbo 算法，但 raster AA、变换/网格拟合时机和缓存结构仍需按场景判断。', '']
lines += ['', '### 独立进程资源压力', '',
'下表是 round15 保存的独立进程 100,000 硬件实例×60帧资源压力证据，未伪装成round17重测。实例payload 2,800,000 bytes、几何144 bytes；Windows working set 高水位与六次GPU内存计数采样均不等于完整驱动VRAM峰值。新外部纹理路径另有多格式/alpha/嵌套缓存回归。', '',
'| 后端 | CPU 总秒 | peak working set MiB | peak private commit MiB | max sampled dedicated/shared/committed MiB |', '|---|---|---|---|---|']
for backend in ['dx12','vulkan','gles']:
    data=json.loads((ROOT/'crates/jadix_2d/evidence/round15/resource-isolated'/backend/'resource-pressure.json').read_text(encoding='utf-8'))
    counters=[]
    for sample in data['samples']:
        raw=sample['gpu'];raw=[raw] if isinstance(raw,dict) else raw
        counters.append({key:sum(v[key] for v in raw) for key in ['DedicatedUsage','SharedUsage','TotalCommitted']})
    maximum=[max(v[key] for v in counters)/1048576 for key in ['DedicatedUsage','SharedUsage','TotalCommitted']]
    end=data['end']
    lines.append(f'| {backend} | {end["cpu_seconds"]:.3f} | {end["peak_working_set_bytes"]/1048576:.2f} | {end["peak_commit_bytes"]/1048576:.2f} | '+ '/'.join(f'{v:.2f}' for v in maximum)+' |')
lines += ['', '上述压力测试通过，但不是所有字体/纹理/离屏图层/无限快照的混合资源压力；30k 普通 Canvas 微几何的失败另见上文。', '']
comparison=HISTORICAL/'vello-comparison.json' 
if comparison.exists():
    value=json.loads(comparison.read_text(encoding='utf-8'))
    lines+=['| 后端/场景/尺寸/Jadix AA | Vello P50/P95 ms | Jadix P50/P95 ms | Vello/Jadix P50 | linear-premul MAE (0..255) |','|---|---|---|---|---|']
    for r in value['cases']:
        lines.append(f'| {r["backend"]}/{r["case"]}/{r["size"]}/{r["jadix_quality"]} | {r["vello_p50_ms"]:.3f}/{r["vello_p95_ms"]:.3f} | {r["jadix_p50_ms"]:.3f}/{r["jadix_p95_ms"]:.3f} | {r["vello_over_jadix_p50"]:.3f} | {r["linear_premultiplied_mae_255"]:.4f} |')
    comparable=[r for r in value['cases'] if r['jadix_quality']=='balanced']
    lines += ['', f'round17 Balanced 的 {len(comparable)} 项中 Jadix 有 {sum(r["vello_over_jadix_p50"]>1 for r in comparable)} 项 P50 较低；只报告本机有限数据，未做重复试验置信区间，也不是全场景结论。AA 与保留缓存工作不同，逐项误差和计时须一起阅读。']
    lines+=['','![round17 规范化场景像素对照：Vello/Jadix Area/差异放大](../crates/jadix_2d/evidence/round17/pixel-comparison.png)', '', '源数据：[vello-comparison.json](../crates/jadix_2d/evidence/round17/vello-comparison.json)。有限场景数据不代表全面超越；完整 API 清单中的缺口不因该基准消失。']
else:lines+=['历史 round17 基准尚未找到；不填造测量值。']
oracle=HISTORICAL/'winding-oracle.json'
if oracle.exists():
    value=json.loads(oracle.read_text(encoding='utf-8'))
    lines+=['','### 重复 nonzero 轮廓的独立覆盖参考','','同一 thin rectangle 重复两次不改变 nonzero 填充区域。Python f64 直接计算矩形与像素交面积，不调用双方几何/栅格实现。这里只验证 alpha coverage，不是所有复杂路径的正确性证明。','','| 后端/尺寸/渲染器 | ink alpha MAE (0..255) | max alpha delta |','|---|---|---|']
    for r in value['cases']:lines.append(f'| {r["backend"]}/{r["size"]}/{r["renderer"]} | {r["ink_alpha_mae_255"]:.4f} | {r["max_alpha_delta_255"]:.4f} |')
    lines+=['','完整参考：[winding-oracle.json](../crates/jadix_2d/evidence/round17/winding-oracle.json)。']
large=HISTORICAL/'large-scene-runs.json'
if large.exists():
    lines+=['','### 30k 真正亚像素矩形重跑','','round17 双方统一0.8×0.8逻辑像素；旧参考曾用1px对应 Jadix 原先的最小1px钳制。旧结果不可与新几何直接混算，失败证据保留。当前 Jadix Area 的 CPU/GPU误差以及所有 Vello失败均按实际记录。','','| 后端/渲染器 | exit | 秒 |','|---|---|---|']
    for r in json.loads(large.read_text(encoding='utf-8')):lines.append(f'| {r["backend"]}/{r["renderer"]} | {r["exit_code"]} | {r["seconds"]} |')
    lines+=['','命令与状态：[large-scene-runs.json](../crates/jadix_2d/evidence/round17/large-scene-runs.json)。两测量帧只作有限压力/画质验证，不用于性能排名。']
clip=ROOT/'crates/jadix_2d/evidence/round18/clip-oracle.json'
if clip.exists():
    value=json.loads(clip.read_text(encoding='utf-8'))
    lines+=['','## round18：几何交集与重复 clip 的独立参考','','32个不重叠细矩形、256²/512²、三个原生后端。Vello 采用两个相同 push_clip_layer；Jadix 显式两次 Intersection 后一次 fill。直接 Python f64 矩形交集/像素面积参考，不调用双方栅格代码。这里只比较 alpha coverage，准备路径和缓存工作不同，不做速度排名。','','| 后端/尺寸/实现 | ink alpha MAE /255 | max alpha delta /255 |','|---|---|---|']
    for r in value['cases']:lines.append(f'| {r["backend"]}/{r["size"]}/{r["renderer"]} | {r["ink_alpha_mae_255"]:.5f} | {r["max_alpha_delta_255"]:.5f} |')
    lines+=['','[原始 oracle 与限制](../crates/jadix_2d/evidence/round18/clip-oracle.json)。新 API 解决显式几何集合，不会自动改变原 clip_path mask 或修复任意重叠 draw 的采样相关性。']
sampling=ROOT/'crates/jadix_2d/evidence/round19/sampling-oracle.json'
if sampling.exists():
    value=json.loads(sampling.read_text(encoding='utf-8'))
    lines+=['','## round19：整场景采样与预滤波缩小的独立参考','','新增公开入口：`Image2D::with_mipmaps`、`mipmapped_region`、`sample_linear_with_footprint`、`ImageSampling::Trilinear/Anisotropic`、`Scene2D::sampled`、`SampledScene2D::rasterize/prepare_gpu`、`PreparedSampledSceneGpu::render`。默认 API 路径不自动切换为高成本超采样。','','32²/64²，三个原生后端。接缝场景为两个在 x=12.25 相接的纯色不透明矩形；缩小场景为383×191黑白棋盘，参考为 Python f64 对原始 texel 面积积分，颜色解码到 linear-premultiplied 后比较。Jadix 分别使用整场景4×4样本和显式 Trilinear mip；Vello 使用 ImageQuality::High，分别运行 Area/Msaa8/Msaa16。算法、缓存和滤波带宽不同，不以这些运行的时间作速度排名。主误差包含颜色域合同差异：固定 Vello fine.wgsl 对 encoded image_atlas texel 做数值插值，Jadix 在 linear-premultiplied 域预滤波；不能把 gamma 差异都解释为混叠。原始 JSON 的 filter_domain_rgb_mae_255 另按各自滤波域计算，以区分缩小振荡与传递函数差异。','','| 后端/尺寸/场景/实现 | linear premul MAE /255 | RMSE (0..255) | max alpha delta /255 |','|---|---|---|---|']
    for r in value['cases']:
        lines.append(f'| {r["backend"]}/{r["size"]}/{r["case"]}/{r["renderer"]} | {r["linear_premultiplied_mae_255"]:.5f} | {r["linear_premultiplied_rmse_255"]:.5f} | {r["max_alpha_delta_255"]:.5f} |')
    lines+=['','![round19：相接色块与 NPOT 高频缩小，对照独立面积参考](../crates/jadix_2d/evidence/round19/sampling-comparison.png)','','[完整原始数据](../crates/jadix_2d/evidence/round19/sampling-oracle.json)、[命令及状态](../crates/jadix_2d/evidence/round19/sampling-reference-runs.json)、[API/限制/跨平台测试](2d-round19.md)。这里的有限画质优势不能外推为全 API、所有几何或全平台超越。', '', 'round19 时 mip 链由调用者显式准备；其 GPU 外部纹理注册只产生 level 0（该缺口已由下述 round20 补齐）。源矩形需要 `mipmapped_region` 独立预滤波，否则按 base bilinear 限域。16-tap 各向异性是有界主轴积分，非 EWA。整场景网格最多64样本/像素，可能漏掉比采样间距更细的几何；FP16中间结果和256..1024MiB预算有明确上限。Metal/mobile、OS HDR 实际呈现、全 CSS color 和所有字体/网格资源混合压力仍未证明。']
lines+=['','## round20：GPU-only mip、Gaussian EWA 与大 compound 路径','','本轮补齐 GPU 外部图像的独立区域预滤波，并增加 CPU/GPU `ImageSampling::Ewa`。`register_texture_region` / `override_image_region` 显式选择源区域和 `MipmapLimits`；`set_registered_texture_mipmaps` 配置已有句柄；`PathGpu::prepare_sampled_scene` 将注册表冻结到整场景准备对象。源纹理不读回，颜色/alpha 转换后在 FP16 mip 链中逐层面积预滤波。脏更新重建，热帧复用；mask 保持单层。','','同色非图像 Area/点模式大路径使用16×16像素空间列表，保留一次像素合成。6400三角形 compound 与整场景采样已在三原生后端通过独立 f64 面积/CPU参考。每列表4096三角形、262144输入三角形、1048560几何texel、约67M work 仍是显式预算；密集重叠或巨大跨域三角形仍可能失败。','','Gaussian EWA 的实际 NPOT 坐标、协方差和归一化支持最多17×17/层、最多两层；省略/截断 mip 时支持范围按8 texel钳制。它与 Vello High 的 Mitchell Bicubic、box 参考和有限主轴采样具有不同滤波带宽，不能仅凭一个误差指标宣布全方面胜出。','','[完整 API、合同、验证与资源限制](2d-round20.md)、[本轮结构化摘要](../crates/jadix_2d/evidence/round20/summary.json)。']
oblique=EVIDENCE/'oblique-oracle.json'
if oblique.exists():
    data=json.loads(oblique.read_text(encoding='utf-8'))
    lines+=['','### 斜向缩小的明确参考','','127×65图像，Repeat/Reflect，相同 affine，32²/64²、三后端。独立 Python f64 对 affine 像素 footprint/source texel 单元裁切作面积积分，另记录32×32 stratified交叉检查与质量守恒误差；这里是距离指定 linear-light box 参考的误差，Gaussian/tent/encoded bicubic 的合同差异全部计入。时间与资源不能由像素误差推导。','','| 后端/尺寸/实现 | linear RGB MAE (0..255) | RMSE (0..255) | max (0..255) |','|---|---|---|---|']
    for r in data['cases']:lines.append(f'| {r["backend"]}/{r["size"]}/{r["renderer"]} | {r["linear_rgb_mae_255"]:.5f} | {r["linear_rgb_rmse_255"]:.5f} | {r["max_error_255"]:.5f} |')
    lines+=['','![斜向缩小：Vello High、Jadix三滤波及独立box参考](../crates/jadix_2d/evidence/round20/oblique-comparison.png)','','[完整原始结果、独立参考交叉检查与合同](../crates/jadix_2d/evidence/round20/oblique-oracle.json)。Metal/mobile、OS HDR 实际呈现、完整 CSS 语法、所有资源混合压力以及 Vello compute ABI 兼容仍未证明；公开 API 枚举完整不代表全 API 动态等价。']

lines+=['','## round21：四项对应入口、DX12 初始化和可选 compute','','上一轮四个 `missing` 项现在均有公开用途对应入口：初始化线程和 wgpu cache 分类为用途对应；CPU 执行和场景估算仍保留 `partial`，因为它们分别采用完整 CPU 光栅化与 Jadix payload 规划，而非 Vello compute coarse/fine 与 bump ABI。消除 `missing` 标签不代表消除所有语义差异。','','| 项目 | 当前实现与边界 |','|---|---|','| CPU 模式 | `SceneRenderer2D` 执行 CPU clip/filter/composite，再上传线性预乘 FP16 并输出 straight/premultiplied。CPU surface 预算默认512MiB、硬上限1GiB；GPU-only 图像需显式 CPU fallback。 |','| 初始化线程 | `RendererOptions::num_init_threads` 控制实际所需材质的有界线程批次，`warm_up` 可主动预热；WebAssembly 串行。 |','| 管线缓存 | caller-owned same-device `wgpu::PipelineCache` 传入 Jadix 自有路径、Area、layer/filter/import/mip/sample-resolve 管线。Vulkan 数据读回实测；DX12 不保证可持久化，独立 `GpuMesh` 不归该配置管理。 |','| 场景估算 | `SceneMemoryEstimate` 检查整数溢出，报告几何、vertex upload、Area列表、去重图像、保守 layer/target 字节及工作量上界。不是总RSS/VRAM峰值或 Vello bump 编码。 |','| DX12 首次使用 | Area 先路由再初始化，只建需要的材质；普通图像模块移除 EWA 函数体，EWA 首次使用单向升级对应 PSO。避免标准/Area 双重无用初始化；没有降低滤波质量。 |','| compute feature | `jadix` / `jadix_2d` / `jadix_renderer` 转发至 `jadix_wgpu/compute`。DX12 backend 不再隐式开启。wgpu 包自身默认 feature 保持上游兼容，工作区依赖仍 default-features=false。 |','| compute 执行 | direct/indirect buffer/storage-texture dispatch，GPU-only NPOT mip compute；WebGL render-pass fallback。CPU/GPU 图像能力不要求 compute。 |','','关闭 compute 时两个安全公开入口 `Device::create_compute_pipeline`、`CommandEncoder::begin_compute_pass` 不可用；描述符/handle 类型及后端内部仍保留，不能称为完整后端剥离或安全隔离。Cargo feature 是依赖图并集，任何依赖显式启用都会在同一包实例生效。真实执行还依赖 requested-device limits 和 adapter downlevel flags。','','[本轮详细合同与验证](2d-round21.md)、[结构化验收](../crates/jadix_2d/evidence/round21/summary.json)、[feature 正反编译](../crates/jadix_2d/evidence/round21/compute-feature-contract.json)。四项公开入口已补齐，但 Metal/mobile 实机、OS HDR 呈现、完整 CSS 语法和所有资源混合高水位仍不在现有证据范围。']
for backend in ['dx12','vulkan','gles']:
    path=CURRENT/backend/'cold-start.json'
    if path.exists():
        cold=json.loads(path.read_text(encoding='utf-8'))
        lines+=['',f'### {backend} 初始化阶段观测（当前 round24）','','| 同设备新 renderer | 构造 ms | 首次纯色 ms | 首次 bilinear 图案 ms | 首次 EWA 图案 ms |','|---|---:|---:|---:|---:|']
        for row in cold['rows']:
            lines.append(f'| {row["iteration"]} | {row["constructor_ms"]:.3f} | {row["first_solid_ms"]:.3f} | {row.get("first_bilinear_pattern_ms",0):.3f} | {row["first_ewa_pattern_ms"]:.3f} |')
        lines += ['', '包含验证、提交和GPU等待；没有清空系统/驱动缓存，不是纯编译计时，不能据此声称全场景冷启动倍率或超过 Vello。']

lines+=['','## round22：compute 命令与资源 API 的逐项对应','','新增 `gpu::low_level`（需要 compute feature），覆盖 ResourceId、BufferProxy、ImageProxy、ResourceProxy/BufferRange、BindType、ShaderId、Command、Recording 和实际 Engine 执行。原先 low_level 的统一“无兼容 ABI”说明已逐项替换为入口、执行语义和具体限制，FullShaders/Render 算法阶段仍保留差异。','','上传采用 encoder staging copy，保留 `upload → dispatch → upload → indirect → download` 的实际顺序；读回是命令位置快照，资源存续至显式释放。检查 ID/descriptor 冲突、use-after-free、range 对齐/长度、图像区域、binding schema、工作组和 resource/staging/encoder峰值预算；无效 recording 预检不分配、不提交。原生 GPU validation 可同步返回错误；浏览器仍需外层 awaited error scope，不能将同步返回当作异步验证成功。','','`Engine::add_compute_shader` 和 `add_render_shader` 接受调用者 WGSL，PSO 按需编译并缓存；同一 command stream 可交错 compute 与无 vertex-buffer 的 vertex-index render draw。支持 FP16 storage 图像、局部上传、image readback（含显式 padded stride）和现有 Canvas GPU import。不是引入 Vello 运行时或复制其内部 buffers。','','`low_level::Render` 把 CPU scene preparation、可取走的 CapturedScene、调用者 encoder 的 record_fine 和保留 FP16 out_image 分开；它接受 Scene2D，不接受 Vello Encoding/Resolver。FullShaders 的 pathtag scans、coarse tile commands、GPU bump allocator 和 native fine_msaa8/16 不伪装成已兼容：对应行逐项写明 native 方案与未实现的特定算法。','','DX12 再增加 nearest/bilinear 专用 shader tier，去掉 Bicubic/Lanczos/mip/EWA 重建循环；高阶采样和 EWA 单向升级对应材质，混合场景保持最强已需 tier。首次普通图案不承担完整重建编译；完整质量矩阵及历史图像继续作为回归门槛。','','[本轮实现、预算与实际验收](2d-round22.md)、[结构化证据](../crates/jadix_2d/evidence/round22/summary.json)。Native low-level 命令用途覆盖不等于 Vello 内部 shader-buffer ABI 兼容，未把算法差异改标为虚假的等价。']

lines+=['','## round23：最新稳定版对照与后续增强','','2026-11-06 通过 crates.io 官方 API 实时查询，Vello 最新稳定版为0.11.0（2026-10-02发布），Vello Hybrid0.2.0、Vello CPU0.3.0是不同包。本轮实际链接发布版 Vello0.11.0/upstream wgpu30，未修改其渲染源码；[原始版本查询](../crates/jadix_2d/evidence/round23/vello-registry.json)与锁文件一并保留。','','新增同输入、同阶段的三个后端对照，每实现/后端三个全新进程，交替执行顺序；每进程同设备新建两个 renderer，Area、单初始化线程，DX12均FXC。原始统一白图/NPOT/斜向图案文件由两实现读取，同样3200矩形compound输入。分别记录构造、CPU准备、render+wait、读回和整个进程CPU/峰值RSS。','','Vello会在构造阶段初始化更多PSO；Jadix延迟初始化，所以“构造+首次纯色”与整个进程都列出。High是Vello High对Jadix EWA，滤波带宽/色域/显式mip不同，不能当作严格同算法速度排名。旧Jadix GPU-only FP16 mip、HDR、sampled-scene整套测试没有对应的Vello公开Scene执行组合，不能拿旧80.98秒直接除以本表的进程时间。']
matched=ROOT/'crates/jadix_2d/evidence/round23/matched-summary.json'
if matched.exists():
    data=json.loads(matched.read_text(encoding='utf-8'))
    lines+=['','### 同输入混合进程：三次观测中位数','','| 后端 | 实现 | 构造+首次纯色 ms | 混合进程 wall s | 进程CPU s | 峰值RSS MiB |','|---|---|---:|---:|---:|---:|']
    for r in data['processes']:lines.append(f'| {r["backend"]} | {r["renderer"]} | {r["constructor_plus_first_solid_ms_median"]:.2f} | {r["wall_seconds_median"]:.4f} | {r["cpu_seconds_median"]:.4f} | {r["peak_rss_mib_median"]:.2f} |')
    lines+=['','### 同阶段 render+GPU wait：首次与热帧中位数','','| 后端 | 实现 | 阶段 | 同设备renderer序号 | 首次 ms | 其余帧中位 ms | CPU准备中位 ms |','|---|---|---|---:|---:|---:|---:|']
    for r in data['phases']:lines.append(f'| {r["backend"]} | {r["renderer"]} | {r["case"]} | {r["iteration"]} | {r["first_render_wait_ms_median"]:.4f} | {r["warm_render_wait_ms_median"]:.4f} | {r["prepare_ms_median"]:.4f} |')
    lines+=['','### 斜向图案：独立线性box参考距离','','| 后端 | 实现 | linear RGB MAE (0..255) | RMSE | max |','|---|---|---:|---:|---:|']
    for r in data['oblique_box_reference']:lines.append(f'| {r["backend"]} | {r["renderer"]} | {r["linear_rgb_mae_255"]:.5f} | {r["linear_rgb_rmse_255"]:.5f} | {r["max_error_255"]:.5f} |')
    lines+=['','![当前Vello High、Jadix EWA和独立参考](../crates/jadix_2d/evidence/round23/matched-oblique.png)','','此参考对源texel单元与affine像素footprint作f64裁切积分，检查面积守恒；这里只评价距离指定linear box参考的差距。Gaussian EWA/encoded bicubic不是相同滤镜；没有从误差推导全API画质胜出。','','[完整对照结果](../crates/jadix_2d/evidence/round23/matched-summary.json)、[18个独立进程观测](../crates/jadix_2d/evidence/round23/matched-runs.json)、[输入/二进制/来源哈希](../crates/jadix_2d/evidence/round23/matched-manifest.json)。']
lines+=['','`Scene2D::reset/reset_and_resize/rerecord` 补充mutable handle的用途对应，并保持已冻结克隆/父场景快照；`Engine::import_buffer/import_image` 接入同设备外部资源，执行前检查实际usage。EWA专用shader不包含Cubic/Lanczos/anisotropic分支，遇到两类高阶采样共存时按能力位集合升级完整shader；旧采样数学和kind=8不变。','','[round23完整合同、未解决差异与实测限制](2d-round23.md)、[最终结构化验收](../crates/jadix_2d/evidence/round23/summary.json)。']


lines+=['','## round24：DX12 编译回归定位、命令包与矩形采样','','三个新DX12进程在修改前首次High均超过6秒。保留原始HLSL直接调用系统FXC确认单个fs_area_pattern约6秒；loop提示、prefer-flow和重复调用合并未改善，均未保留。有效改动为整数寻址改单次unsigned remainder并恢复负数Euclidean域、统一mip bilinear的零级与其他级路径；纯色Area入口直接计算纯色，避免编译不使用的gradient/Gaussian通用路径。没有改变编译器选项或改用低质量滤镜。','','`Engine::warm_up` 只预编译有效Recording引用的PSO，不分配命令资源、不上传、不提交；`Engine::record` 返回带command buffer/downloads/stats的`RecordedExecution`，允许调用者在同一提交中插入前后GPU命令。编码成功后资源逻辑状态前进；须按record顺序提交，丢弃命令包后需重置相关资源。原execute仍是record+submit便捷入口，浏览器仍需要awaited error scope。','','`Scene2D::sampled_grid`支持每轴1/2/4/8，覆盖总1/2/4/8/16/32/64采样；CPU和GPU保持每样本clip/composite后再resolve，Gaussian、offset、morphology、convolution按XY各自缩放。2x4和4x2为实际8样本用途，不伪装为Vello硬件MSAA8或fine compute ABI。']
matched24=CURRENT/'matched-summary.json'
if matched24.exists():
    data=json.loads(matched24.read_text(encoding='utf-8'))
    lines+=['','### 当前同场景进程观测：每实现/后端三个新进程','','| 后端 | 实现 | 构造+首次纯色 ms | 进程wall s | 进程CPU s | 峰值RSS MiB |','|---|---|---:|---:|---:|---:|']
    for r in data['processes']:lines.append(f'| {r["backend"]} | {r["renderer"]} | {r["constructor_plus_first_solid_ms_median"]:.2f} | {r["wall_seconds_median"]:.4f} | {r["cpu_seconds_median"]:.4f} | {r["peak_rss_mib_median"]:.2f} |')
    lines+=['','### 当前阶段延迟：与进程wall分别保留','','| 后端 | 实现 | 阶段 | renderer序号 | 首次 ms | 热帧中位 ms | CPU准备中位 ms |','|---|---|---|---:|---:|---:|---:|']
    for r in data['phases']:lines.append(f'| {r["backend"]} | {r["renderer"]} | {r["case"]} | {r["iteration"]} | {r["first_render_wait_ms_median"]:.4f} | {r["warm_render_wait_ms_median"]:.4f} | {r["prepare_ms_median"]:.4f} |')
    lines+=['','三后端同原始输入、Area、单初始化线程、每设备两个renderer、交替三个进程；High滤镜仍分别为Vello High与Jadix EWA，不是严格同算法吞吐排名。缓存未清，不将三个样本外推所有设备。GPU-only HDR/mip/sampled-scene套件另外记录，不与公共六场景混算。','','[当前完整原始阶段与资源数据](../crates/jadix_2d/evidence/round24/matched-summary.json)、[输入/二进制/源码身份](../crates/jadix_2d/evidence/round24/matched-manifest.json)。']
lines+=['','本轮108张Jadix公共场景PNG及480张历史PNG逐字节不变。未修改的Vello二进制有12张DX12/Vulkan compound输出跨进程/轮次出现最大8级通道差，原因未定位，不计作Jadix回归，也不用于精确像素等价结论。单像素/条带目标现在支持五种AA和采样输出；共72个尺寸/alpha/layer/HDR组合通过。','','[round24实现、回归与性能合同](2d-round24.md)、[最终验收](../crates/jadix_2d/evidence/round24/summary.json)。']

lines+=['','## 可复现性与一手来源','',
'`python benchmarks/vello-api/scan.py` 从原始 rustdoc 枚举；`python benchmarks/vello-api/report.py` 生成本文，未分类 Vello 自有项会使脚本失败。当前源码与 JSON 的 SHA-256 保留于 evidence/round24 和 inventory（round20 数据作为明确标注的历史参考）；历史画质/性能表仍明确链接 round17。不同 target/features 需要重生成，不能将当前清单外推未来版本。', '',
'- [Vello 0.11.0 官方文档](https://docs.rs/vello/0.11.0/vello/)',
'- [Scene](https://docs.rs/vello/0.11.0/vello/struct.Scene.html)',
'- [Renderer](https://docs.rs/vello/0.11.0/vello/struct.Renderer.html)',
'- [DrawGlyphs](https://docs.rs/vello/0.11.0/vello/struct.DrawGlyphs.html)',
'- [Color 0.3.3](https://docs.rs/color/0.3.3/color/)',
'- [Moxcms 0.8.1](https://docs.rs/moxcms/0.8.1/moxcms/)',
'- [CSS Color 4 gamut mapping](https://www.w3.org/TR/css-color-4/#css-gamut-mapping)', '']
OUT.write_text('\n'.join(lines),encoding='utf-8')
print(OUT,len(rows),dict(counts),'lines',len(lines))
