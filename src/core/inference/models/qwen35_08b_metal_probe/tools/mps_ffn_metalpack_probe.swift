import Foundation
import Metal
import MetalPerformanceShaders

struct Entry {
    let tensor: String
    let cls: String
    let layer: Int
    let sourceShape: [Int]
    let layout: String
    let rowTile: Int
    let colTile: Int
    let packedOffset: Int
    let packedBytes: Int
}

func argInt(_ index: Int, _ fallback: Int) -> Int {
    guard CommandLine.arguments.count > index, let value = Int(CommandLine.arguments[index]) else {
        return fallback
    }
    return value
}

func roundUp(_ value: Int, _ multiple: Int) -> Int {
    ((value + multiple - 1) / multiple) * multiple
}

func alignedRowBytes(_ columns: Int, _ elementBytes: Int) -> Int {
    let raw = columns * elementBytes
    return ((raw + 127) / 128) * 128
}

func parseEntries(_ manifestPath: URL) throws -> (URL, [Entry]) {
    let data = try Data(contentsOf: manifestPath)
    guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          let weightsFile = root["weights_file"] as? String,
          let entriesJson = root["entries"] as? [[String: Any]] else {
        throw NSError(domain: "manifest", code: 1, userInfo: [NSLocalizedDescriptionKey: "invalid manifest"])
    }
    let weightsURL = manifestPath.deletingLastPathComponent().appendingPathComponent(weightsFile)
    let entries = entriesJson.compactMap { item -> Entry? in
        guard let tensor = item["tensor"] as? String,
              let cls = item["class"] as? String,
              let shape = item["source_shape"] as? [Int],
              let layout = item["layout"] as? String,
              let rowTile = item["row_tile"] as? Int,
              let colTile = item["col_tile"] as? Int,
              let packedOffset = item["packed_offset"] as? Int,
              let packedBytes = item["packed_bytes"] as? Int else {
            return nil
        }
        let layer = item["layer"] as? Int ?? -1
        return Entry(
            tensor: tensor,
            cls: cls,
            layer: layer,
            sourceShape: shape,
            layout: layout,
            rowTile: rowTile,
            colTile: colTile,
            packedOffset: packedOffset,
            packedBytes: packedBytes
        )
    }
    return (weightsURL, entries)
}

func fp16RowTiledValue(_ weights: Data, _ entry: Entry, _ row: Int, _ col: Int) -> UInt16 {
    let rows = entry.sourceShape[0]
    let cols = entry.sourceShape[1]
    if row >= rows || col >= cols {
        return 0
    }
    let nColTiles = roundUp(cols, entry.colTile) / entry.colTile
    let rowBlock = row / entry.rowTile
    let localRow = row % entry.rowTile
    let colBlock = col / entry.colTile
    let localCol = col % entry.colTile
    let tileElements = entry.rowTile * entry.colTile
    let tileIndex = rowBlock * nColTiles + colBlock
    let elementIndex = tileIndex * tileElements + localRow * entry.colTile + localCol
    let byteIndex = entry.packedOffset + elementIndex * 2
    return weights.withUnsafeBytes { raw in
        let ptr = raw.bindMemory(to: UInt8.self)
        return UInt16(ptr[byteIndex]) | (UInt16(ptr[byteIndex + 1]) << 8)
    }
}

if CommandLine.arguments.count < 2 {
    fputs("usage: mps_ffn_metalpack_probe <metalpack-dir> [layer] [tokens] [iterations] [warmup]\n", stderr)
    exit(2)
}

let metalpack = URL(fileURLWithPath: CommandLine.arguments[1])
let layer = argInt(2, 0)
let tokens = argInt(3, 4096)
let iterations = argInt(4, 3)
let warmup = argInt(5, 1)
let hidden = 1024
let intermediate = 3584
let elementBytes = MemoryLayout<UInt16>.stride

let (weightsURL, entries) = try parseEntries(metalpack.appendingPathComponent("manifest.json"))
let weights = try Data(contentsOf: weightsURL)

func find(_ cls: String) -> Entry {
    guard let entry = entries.first(where: { $0.layer == layer && $0.cls == cls }) else {
        fatalError("missing class \(cls) layer \(layer)")
    }
    guard entry.layout == "fp16_row_tiled" else {
        fatalError("unsupported layout \(entry.layout) for \(entry.tensor)")
    }
    return entry
}

let gateEntry = find("mlp_gate")
let upEntry = find("mlp_up")
let downEntry = find("mlp_down")

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
let guWeightRowBytes = alignedRowBytes(intermediate * 2, elementBytes)
let guRowBytes = alignedRowBytes(intermediate * 2, elementBytes)
let actRowBytes = alignedRowBytes(intermediate, elementBytes)
let downWeightRowBytes = alignedRowBytes(hidden, elementBytes)
let outRowBytes = alignedRowBytes(hidden, elementBytes)

let xBuffer = makeBuffer(tokens * xRowBytes)
let guWeightBuffer = makeBuffer(hidden * guWeightRowBytes)
let guBuffer = makeBuffer(tokens * guRowBytes)
let actBuffer = makeBuffer(tokens * actRowBytes)
let downWeightBuffer = makeBuffer(intermediate * downWeightRowBytes)
let outBuffer = makeBuffer(tokens * outRowBytes)

func fillHalfBuffer(_ buffer: MTLBuffer, byteCount: Int, seed: UInt32) {
    let ptr = buffer.contents().bindMemory(to: UInt16.self, capacity: byteCount / elementBytes)
    var x = seed
    for i in 0..<(byteCount / elementBytes) {
        x = 1664525 &* x &+ 1013904223
        let mantissa = UInt16((x >> 15) & 0x01ff)
        ptr[i] = UInt16(0x3800) | mantissa
    }
}

fillHalfBuffer(xBuffer, byteCount: tokens * xRowBytes, seed: 0x12345678)

let guPtr = guWeightBuffer.contents().bindMemory(to: UInt16.self, capacity: hidden * guWeightRowBytes / elementBytes)
for k in 0..<hidden {
    let rowBase = k * (guWeightRowBytes / elementBytes)
    for n in 0..<intermediate {
        guPtr[rowBase + n] = fp16RowTiledValue(weights, gateEntry, n, k)
        guPtr[rowBase + intermediate + n] = fp16RowTiledValue(weights, upEntry, n, k)
    }
}

let downPtr = downWeightBuffer.contents().bindMemory(to: UInt16.self, capacity: intermediate * downWeightRowBytes / elementBytes)
for k in 0..<intermediate {
    let rowBase = k * (downWeightRowBytes / elementBytes)
    for n in 0..<hidden {
        downPtr[rowBase + n] = fp16RowTiledValue(weights, downEntry, n, k)
    }
}

let x = MPSMatrix(buffer: xBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: xRowBytes, dataType: .float16))
let guWeight = MPSMatrix(buffer: guWeightBuffer, descriptor: MPSMatrixDescriptor(rows: hidden, columns: intermediate * 2, rowBytes: guWeightRowBytes, dataType: .float16))
let gu = MPSMatrix(buffer: guBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: intermediate * 2, rowBytes: guRowBytes, dataType: .float16))
let act = MPSMatrix(buffer: actBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: intermediate, rowBytes: actRowBytes, dataType: .float16))
let downWeight = MPSMatrix(buffer: downWeightBuffer, descriptor: MPSMatrixDescriptor(rows: intermediate, columns: hidden, rowBytes: downWeightRowBytes, dataType: .float16))
let out = MPSMatrix(buffer: outBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: outRowBytes, dataType: .float16))

let gateUpGemm = MPSMatrixMultiplication(device: device, transposeLeft: false, transposeRight: false, resultRows: tokens, resultColumns: intermediate * 2, interiorColumns: hidden, alpha: 1.0, beta: 0.0)
let downGemm = MPSMatrixMultiplication(device: device, transposeLeft: false, transposeRight: false, resultRows: tokens, resultColumns: hidden, interiorColumns: intermediate, alpha: 1.0, beta: 0.0)

let source = """
#include <metal_stdlib>
using namespace metal;
kernel void swiglu_kernel(device const half* gate_up [[buffer(0)]], device half* act [[buffer(1)]], constant uint& intermediate [[buffer(2)]], constant uint& gu_stride [[buffer(3)]], constant uint& act_stride [[buffer(4)]], uint2 gid [[thread_position_in_grid]]) {
    const uint token = gid.y;
    const uint col = gid.x;
    if (col >= intermediate) return;
    const uint gu_base = token * gu_stride;
    const uint act_base = token * act_stride;
    const float gate = float(gate_up[gu_base + col]);
    const float up = float(gate_up[gu_base + intermediate + col]);
    const float sigmoid = 1.0f / (1.0f + exp(-gate));
    act[act_base + col] = half((gate * sigmoid) * up);
}
"""
let library = try device.makeLibrary(source: source, options: nil)
let swiglu = try device.makeComputePipelineState(function: library.makeFunction(name: "swiglu_kernel")!)
var intermediateU32 = UInt32(intermediate)
var guStrideU32 = UInt32(guRowBytes / elementBytes)
var actStrideU32 = UInt32(actRowBytes / elementBytes)

func encodeSwiglu(_ commandBuffer: MTLCommandBuffer) {
    let encoder = commandBuffer.makeComputeCommandEncoder()!
    encoder.setComputePipelineState(swiglu)
    encoder.setBuffer(guBuffer, offset: 0, index: 0)
    encoder.setBuffer(actBuffer, offset: 0, index: 1)
    encoder.setBytes(&intermediateU32, length: MemoryLayout<UInt32>.stride, index: 2)
    encoder.setBytes(&guStrideU32, length: MemoryLayout<UInt32>.stride, index: 3)
    encoder.setBytes(&actStrideU32, length: MemoryLayout<UInt32>.stride, index: 4)
    encoder.dispatchThreads(MTLSize(width: intermediate, height: tokens, depth: 1), threadsPerThreadgroup: MTLSize(width: 256, height: 1, depth: 1))
    encoder.endEncoding()
}

func runOnce() -> Double {
    let commandBuffer = queue.makeCommandBuffer()!
    let start = DispatchTime.now().uptimeNanoseconds
    gateUpGemm.encode(commandBuffer: commandBuffer, leftMatrix: x, rightMatrix: guWeight, resultMatrix: gu)
    encodeSwiglu(commandBuffer)
    downGemm.encode(commandBuffer: commandBuffer, leftMatrix: act, rightMatrix: downWeight, resultMatrix: out)
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
let flops = 2.0 * Double(tokens) * Double(hidden) * Double(intermediate * 2) + 2.0 * Double(tokens) * Double(intermediate) * Double(hidden)
var checksum: UInt64 = 0
let outPtr = outBuffer.contents().bindMemory(to: UInt16.self, capacity: tokens * outRowBytes / elementBytes)
for i in 0..<min(16, tokens * outRowBytes / elementBytes) { checksum &+= UInt64(outPtr[i]) }

print("mps_ffn_metalpack_probe")
print("metalpack: \(metalpack.path)")
print("layer: \(layer)")
print("device: \(device.name)")
print("shape: tokens=\(tokens) hidden=\(hidden) intermediate=\(intermediate)")
print("backend: real metalpack weights -> MPSMatrix + MSL SwiGLU")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", flops / median / 1.0e12))
print("checksum16: \(checksum)")
