import Foundation
import Metal
import MetalPerformanceShaders

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

func fillHalfBuffer(_ buffer: MTLBuffer, byteCount: Int, seed: UInt32) {
    let ptr = buffer.contents().bindMemory(to: UInt16.self, capacity: byteCount / 2)
    var x = seed
    for i in 0..<(byteCount / 2) {
        x = 1664525 &* x &+ 1013904223
        let mantissa = UInt16((x >> 13) & 0x03ff)
        ptr[i] = UInt16(0x3c00) | mantissa
    }
}

func percentile(_ sorted: [Double], _ q: Double) -> Double {
    if sorted.isEmpty { return 0.0 }
    let idx = min(sorted.count - 1, Int(Double(sorted.count - 1) * q + 0.5))
    return sorted[idx]
}

let tokens = argInt(1, 4096)
let qTile = argInt(2, 128)
let kTile = argInt(3, 512)
let iterations = argInt(4, 5)
let warmup = argInt(5, 1)
let headDim = 256

guard tokens > 0, qTile > 0, kTile > 0, iterations > 0 else {
    fatalError("tokens, qTile, kTile, and iterations must be > 0")
}
guard let device = MTLCreateSystemDefaultDevice() else {
    fatalError("MTLCreateSystemDefaultDevice returned nil")
}
guard MPSSupportsMTLDevice(device) else {
    fatalError("MPS does not support this Metal device")
}
guard let queue = device.makeCommandQueue() else {
    fatalError("failed to create command queue")
}

let elementBytes = MemoryLayout<UInt16>.stride
let qRowBytes = alignedRowBytes(headDim, elementBytes)
let kRowBytes = alignedRowBytes(kTile, elementBytes)
let scoreRowBytes = alignedRowBytes(kTile, elementBytes)
let qBytes = qTile * qRowBytes
let kBytes = headDim * kRowBytes
let scoreBytes = qTile * scoreRowBytes

guard let qBuffer = device.makeBuffer(length: qBytes, options: .storageModeShared),
      let kBuffer = device.makeBuffer(length: kBytes, options: .storageModeShared),
      let scoreBuffer = device.makeBuffer(length: scoreBytes, options: .storageModeShared) else {
    fatalError("failed to allocate buffers")
}
fillHalfBuffer(qBuffer, byteCount: qBytes, seed: 0x12345678)
fillHalfBuffer(kBuffer, byteCount: kBytes, seed: 0x9abcdef0)

let qDesc = MPSMatrixDescriptor(rows: qTile, columns: headDim, rowBytes: qRowBytes, dataType: .float16)
let kDesc = MPSMatrixDescriptor(rows: headDim, columns: kTile, rowBytes: kRowBytes, dataType: .float16)
let scoreDesc = MPSMatrixDescriptor(rows: qTile, columns: kTile, rowBytes: scoreRowBytes, dataType: .float16)
let q = MPSMatrix(buffer: qBuffer, descriptor: qDesc)
let k = MPSMatrix(buffer: kBuffer, descriptor: kDesc)
let score = MPSMatrix(buffer: scoreBuffer, descriptor: scoreDesc)

let qBlocks = (tokens + qTile - 1) / qTile
let kBlocks = (tokens + kTile - 1) / kTile
var causalTilePairs = 0
for qb in 0..<qBlocks {
    let qLast = min((qb + 1) * qTile, tokens) - 1
    causalTilePairs += min(kBlocks, qLast / kTile + 1)
}

let gemm = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: qTile,
    resultColumns: kTile,
    interiorColumns: headDim,
    alpha: 1.0,
    beta: 0.0
)

func runOnce() -> Double {
    guard let commandBuffer = queue.makeCommandBuffer() else {
        fatalError("failed to create command buffer")
    }
    let start = DispatchTime.now().uptimeNanoseconds
    for _ in 0..<causalTilePairs {
        gemm.encode(commandBuffer: commandBuffer, leftMatrix: q, rightMatrix: k, resultMatrix: score)
    }
    commandBuffer.commit()
    commandBuffer.waitUntilCompleted()
    if let error = commandBuffer.error {
        fatalError("MPS command buffer failed: \(error)")
    }
    let end = DispatchTime.now().uptimeNanoseconds
    return Double(end - start) / 1_000_000_000.0
}

if warmup > 0 {
    for _ in 0..<warmup {
        _ = runOnce()
    }
}

var samples: [Double] = []
samples.reserveCapacity(iterations)
for _ in 0..<iterations {
    samples.append(runOnce())
}
samples.sort()

let median = samples[samples.count / 2]
let p95 = percentile(samples, 0.95)
let flopsPerTile = 2.0 * Double(qTile) * Double(kTile) * Double(headDim)
let totalFlops = flopsPerTile * Double(causalTilePairs)
let tflops = totalFlops / median / 1.0e12
let scoreMiB = Double(scoreBytes) / (1024.0 * 1024.0)
let qMiB = Double(qBytes) / (1024.0 * 1024.0)
let kMiB = Double(kBytes) / (1024.0 * 1024.0)

print("tiled_attention_qk_mps_prototype")
print("device: \(device.name)")
print("tokens: \(tokens)")
print("q_tile: \(qTile)")
print("k_tile: \(kTile)")
print("head_dim: \(headDim)")
print("q_blocks: \(qBlocks)")
print("k_blocks: \(kBlocks)")
print("causal_tile_pairs: \(causalTilePairs)")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print("contract: synthetic repeated QK tile GEMM; no softmax/V and no real Q/K slicing")
print(String(format: "q_tile_mib: %.3f", qMiB))
print(String(format: "k_tile_mib: %.3f", kMiB))
print(String(format: "score_tile_mib: %.3f", scoreMiB))
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", tflops))
print(String(format: "mps_encodes_per_s: %.3f", Double(causalTilePairs) / median))
