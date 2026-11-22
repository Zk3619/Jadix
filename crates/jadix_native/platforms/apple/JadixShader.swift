import Foundation
import MetalKit
// MTKView presents a CAMetalLayer drawable directly. Rendering never calls
// getBytes/readPixels or creates an NSImage, including animated shaders.
@MainActor final class JadixHostedShader: NSObject, MTKViewDelegate {
    let view = MTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
    var reportError: ((String) -> Void)?
    private var pipeline: MTLRenderPipelineState?
    private var queue: MTLCommandQueue?
    private var source = ""
    private var function = ""
    private var uniforms: [Float] = []
    private let started = ProcessInfo.processInfo.systemUptime
    private let inflight = DispatchSemaphore(value: 3)
    private var visibility: Timer?
    override init() {
        super.init(); queue = view.device?.makeCommandQueue(); view.delegate = self
        view.colorPixelFormat = .bgra8Unorm; view.framebufferOnly = true; view.isPaused = true
        visibility = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.view.isPaused = !jadixSurfaceVisible(self.view) || self.pipeline == nil
        }
    }
    func update(_ value: [String: Any]) {
        uniforms = (value["uniforms"] as? [NSNumber] ?? []).map(\.floatValue)
        view.preferredFramesPerSecond = min(120, max(1, value["frames_per_second"] as? Int ?? 60))
        let nextSource = value["source"] as? String ?? "", nextFunction = value["function"] as? String ?? ""
        if nextSource == source && nextFunction == function { return }
        source = nextSource; function = nextFunction; pipeline = nil
        guard let device = view.device else { reportError?("Metal is unavailable"); return }
        do {
            let vertex = """
            #include <metal_stdlib>
            using namespace metal;
            struct JadixVertex { float4 position [[position]]; float2 uv; };
            vertex JadixVertex jadix_surface_vertex(uint id [[vertex_id]]) {
                float2 p = float2((id << 1) & 2, id & 2);
                return {float4(p * float2(2,-2) + float2(-1,1),0,1),p};
            }
            """
            let library = try device.makeLibrary(source: vertex + "\n" + source, options: nil)
            let descriptor = MTLRenderPipelineDescriptor()
            descriptor.vertexFunction = library.makeFunction(name: "jadix_surface_vertex")
            descriptor.fragmentFunction = library.makeFunction(name: function)
            descriptor.colorAttachments[0].pixelFormat = view.colorPixelFormat
            pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
        } catch { reportError?(String(error.localizedDescription.prefix(4096))) }
    }
    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
    func draw(in view: MTKView) {
        guard let pipeline, inflight.wait(timeout: .now()) == .success else { return }
        guard let drawable = view.currentDrawable, let pass = view.currentRenderPassDescriptor,
              let command = queue?.makeCommandBuffer(), let encoder = command.makeRenderCommandEncoder(descriptor: pass) else { inflight.signal(); return }
        encoder.setRenderPipelineState(pipeline)
        var values = uniforms.isEmpty ? [Float(0)] : uniforms
        values.withUnsafeMutableBytes { bytes in if let base = bytes.baseAddress { encoder.setFragmentBytes(base, length: bytes.count, index: 0) } }
        var frame = SIMD4<Float>(Float(view.drawableSize.width), Float(view.drawableSize.height), Float(ProcessInfo.processInfo.systemUptime - started), 0)
        encoder.setFragmentBytes(&frame, length: MemoryLayout<SIMD4<Float>>.stride, index: 1)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3); encoder.endEncoding()
        let semaphore = inflight; command.addCompletedHandler { _ in semaphore.signal() }
        command.present(drawable); command.commit()
    }
    func dispose() { visibility?.invalidate(); visibility = nil; view.isPaused = true; view.delegate = nil; pipeline = nil; queue = nil }
}

