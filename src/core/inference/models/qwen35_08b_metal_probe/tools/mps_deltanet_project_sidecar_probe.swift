import Foundation
import Metal
import MetalPerformanceShaders

struct LayerEntry {
    let layer: Int
    let qkvzOffset: Int
    let qkvzBytes: Int
}

func argInt(_ index: Int, _ fallback: Int) -> Int {
    guard CommandLine.arguments.count > index, let value = Int(CommandLine.arguments[index]) else {
        return fallback
    }
    return value
}

func alignedRowBytes(_ columns: Int, _ elementBytes: Int) -> Int {
    let raw = columns * elementBytes
    return ((raw + 127) / 128) * 128
}

func parseManifest(_ manifestPath: URL) throws -> (URL, Int, Int, Int, [LayerEntry]) {
    let data = try Data(contentsOf: manifestPath)
    guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          let format = root["format"] as? String,
          format == "ctox.qwen35_08b.mps_delta_project_sidecar",
          let weightsFile = root["weights_file"] as? String,
          let shape = root["shape"] as? [String: Any],
          let hidden = shape["hidden_size"] as? Int,
          let qkvRows = shape["qkv_rows"] as? Int,
          let zRows = shape["z_rows"] as? Int,
          let entriesJson = root["entries"] as? [[String: Any]] else {
        throw NSError(domain: "sidecar", code: 1, userInfo: [NSLocalizedDescriptionKey: "invalid DeltaNet project sidecar manifest"])
    }
    let entries = try entriesJson.map { item -> LayerEntry in
        guard let layer = item["layer"] as? Int,
              let qkvz = item["qkvz"] as? [String: Any],
              let qkvzOffset = qkvz["offset"] as? Int,
              let qkvzBytes = qkvz["bytes"] as? Int else {
            throw NSError(domain: "sidecar", code: 2, userInfo: [NSLocalizedDescriptionKey: "invalid DeltaNet project sidecar entry"])
        }
        return LayerEntry(layer: layer, qkvzOffset: qkvzOffset, qkvzBytes: qkvzBytes)
    }
    return (manifestPath.deletingLastPathComponent().appendingPathComponent(weightsFile), hidden, qkvRows, zRows, entries)
}

if CommandLine.arguments.count < 2 {
    fputs("usage: mps_deltanet_project_sidecar_probe <mps-delta-project-sidecar-dir> [layer] [tokens] [iterations] [warmup] [output-f32 0|1]\n", stderr)
    exit(2)
}

let sidecar = URL(fileURLWithPath: CommandLine.arguments[1])
let layer = argInt(2, 0)
let tokens = argInt(3, 4096)
let iterations = argInt(4, 3)
let warmup = argInt(5, 1)
let outputF32 = argInt(6, 0) != 0
let elementBytes = MemoryLayout<UInt16>.stride
let outputElementBytes = outputF32 ? MemoryLayout<Float>.stride : MemoryLayout<UInt16>.stride

let (weightsURL, hidden, qkvRows, zRows, entries) = try parseManifest(sidecar.appendingPathComponent("manifest.json"))
let combinedRows = qkvRows + zRows
guard let entry = entries.first(where: { $0.layer == layer }) else {
    fatalError("missing layer \(layer) in sidecar")
}
let weights = try Data(contentsOf: weightsURL)

guard let device = MTLCreateSystemDefaultDevice(),
      MPSSupportsMTLDevice(device),
      let queue = device.makeCommandQueue() else {
    fatalError("failed to initialize Metal/MPS")
}

func makeBuffer(_ length: Int) -> MTLBuffer {
    guard let buffer = device.makeBuffer(length: length, options: .storageModeShared) else {
        fatalError("failed to allocate \(length) bytes")
    }
    return buffer
}

let xRowBytes = alignedRowBytes(hidden, elementBytes)
let wRowBytes = alignedRowBytes(combinedRows, elementBytes)
let yRowBytes = alignedRowBytes(combinedRows, outputElementBytes)
let xBytes = tokens * xRowBytes
let yBytes = tokens * yRowBytes

let xBuffer = makeBuffer(xBytes)
let wBuffer = makeBuffer(entry.qkvzBytes)
let yBuffer = makeBuffer(yBytes)

func fillHalfBuffer(_ buffer: MTLBuffer, byteCount: Int, seed: UInt32) {
    let ptr = buffer.contents().bindMemory(to: UInt16.self, capacity: byteCount / elementBytes)
    var x = seed
    for i in 0..<(byteCount / elementBytes) {
        x = 1664525 &* x &+ 1013904223
        let mantissa = UInt16((x >> 15) & 0x01ff)
        ptr[i] = UInt16(0x3800) | mantissa
    }
}

fillHalfBuffer(xBuffer, byteCount: xBytes, seed: 0x12345678)
weights.withUnsafeBytes { raw in
    let src = raw.bindMemory(to: UInt8.self)
    memcpy(wBuffer.contents(), src.baseAddress!.advanced(by: entry.qkvzOffset), entry.qkvzBytes)
}

let x = MPSMatrix(buffer: xBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: xRowBytes, dataType: .float16))
let w = MPSMatrix(buffer: wBuffer, descriptor: MPSMatrixDescriptor(rows: hidden, columns: combinedRows, rowBytes: wRowBytes, dataType: .float16))
let y = MPSMatrix(buffer: yBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: combinedRows, rowBytes: yRowBytes, dataType: outputF32 ? .float32 : .float16))

let gemm = MPSMatrixMultiplication(device: device, transposeLeft: false, transposeRight: false, resultRows: tokens, resultColumns: combinedRows, interiorColumns: hidden, alpha: 1.0, beta: 0.0)

func runOnce() -> Double {
    let commandBuffer = queue.makeCommandBuffer()!
    let start = DispatchTime.now().uptimeNanoseconds
    gemm.encode(commandBuffer: commandBuffer, leftMatrix: x, rightMatrix: w, resultMatrix: y)
    commandBuffer.commit()
    commandBuffer.waitUntilCompleted()
    if let error = commandBuffer.error {
        fatalError("command buffer failed: \(error)")
    }
    return Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000_000.0
}

for _ in 0..<warmup { _ = runOnce() }
var samples: [Double] = []
for _ in 0..<iterations { samples.append(runOnce()) }
samples.sort()
let median = samples[samples.count / 2]
let p95 = samples[min(samples.count - 1, Int(Double(samples.count - 1) * 0.95))]
let flops = 2.0 * Double(tokens) * Double(hidden) * Double(combinedRows)
let visibleBytes = Double(xBytes + entry.qkvzBytes + yBytes)
var checksum: UInt64 = 0
if outputF32 {
    let yPtr = yBuffer.contents().bindMemory(to: Float.self, capacity: yBytes / outputElementBytes)
    for i in 0..<min(16, yBytes / outputElementBytes) { checksum &+= UInt64(abs(yPtr[i]).bitPattern & 0xffff) }
} else {
    let yPtr = yBuffer.contents().bindMemory(to: UInt16.self, capacity: yBytes / outputElementBytes)
    for i in 0..<min(16, yBytes / outputElementBytes) { checksum &+= UInt64(yPtr[i]) }
}

print("mps_deltanet_project_sidecar_probe")
print("sidecar: \(sidecar.path)")
print("layer: \(layer)")
print("device: \(device.name)")
print("shape: tokens=\(tokens) hidden=\(hidden) qkv_rows=\(qkvRows) z_rows=\(zRows) combined_rows=\(combinedRows)")
print("backend: MPS DeltaNet QKV+Z sidecar weights -> MPSMatrix")
print("output_dtype: \(outputF32 ? "float32" : "float16")")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", flops / median / 1.0e12))
print(String(format: "visible_gb_s: %.3f", visibleBytes / median / 1.0e9))
print("checksum16: \(checksum)")
