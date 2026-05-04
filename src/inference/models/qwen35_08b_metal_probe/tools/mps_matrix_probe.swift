import Foundation
import Metal
import MetalPerformanceShaders

func argInt(_ index: Int, _ fallback: Int) -> Int {
    guard CommandLine.arguments.count > index, let value = Int(CommandLine.arguments[index]) else {
        return fallback
    }
    return value
}

let m = argInt(1, 512)
let n = argInt(2, 3584)
let k = argInt(3, 1024)
let iterations = argInt(4, 20)
let warmup = argInt(5, 5)

guard let device = MTLCreateSystemDefaultDevice() else {
    fatalError("MTLCreateSystemDefaultDevice returned nil")
}
guard MPSSupportsMTLDevice(device) else {
    fatalError("MPS does not support this Metal device")
}
guard let queue = device.makeCommandQueue() else {
    fatalError("failed to create command queue")
}

func alignedRowBytes(_ columns: Int, _ elementBytes: Int) -> Int {
    let raw = columns * elementBytes
    return ((raw + 127) / 128) * 128
}

let elementBytes = MemoryLayout<UInt16>.stride
let aRowBytes = alignedRowBytes(k, elementBytes)
let bRowBytes = alignedRowBytes(n, elementBytes)
let cRowBytes = alignedRowBytes(n, elementBytes)

let aBytes = m * aRowBytes
let bBytes = k * bRowBytes
let cBytes = m * cRowBytes

guard let aBuffer = device.makeBuffer(length: aBytes, options: .storageModeShared),
      let bBuffer = device.makeBuffer(length: bBytes, options: .storageModeShared),
      let cBuffer = device.makeBuffer(length: cBytes, options: .storageModeShared) else {
    fatalError("failed to allocate buffers")
}

func fillHalfBuffer(_ buffer: MTLBuffer, byteCount: Int, seed: UInt32) {
    let ptr = buffer.contents().bindMemory(to: UInt16.self, capacity: byteCount / elementBytes)
    var x = seed
    for i in 0..<(byteCount / elementBytes) {
        x = 1664525 &* x &+ 1013904223
        let mantissa = UInt16((x >> 13) & 0x03ff)
        ptr[i] = UInt16(0x3c00) | mantissa
    }
}

fillHalfBuffer(aBuffer, byteCount: aBytes, seed: 0x12345678)
fillHalfBuffer(bBuffer, byteCount: bBytes, seed: 0x9abcdef0)

let aDesc = MPSMatrixDescriptor(rows: m, columns: k, rowBytes: aRowBytes, dataType: .float16)
let bDesc = MPSMatrixDescriptor(rows: k, columns: n, rowBytes: bRowBytes, dataType: .float16)
let cDesc = MPSMatrixDescriptor(rows: m, columns: n, rowBytes: cRowBytes, dataType: .float16)

let a = MPSMatrix(buffer: aBuffer, descriptor: aDesc)
let b = MPSMatrix(buffer: bBuffer, descriptor: bDesc)
let c = MPSMatrix(buffer: cBuffer, descriptor: cDesc)

let gemm = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: m,
    resultColumns: n,
    interiorColumns: k,
    alpha: 1.0,
    beta: 0.0
)

func runOnce() -> Double {
    guard let commandBuffer = queue.makeCommandBuffer() else {
        fatalError("failed to create command buffer")
    }
    let start = DispatchTime.now().uptimeNanoseconds
    gemm.encode(commandBuffer: commandBuffer, leftMatrix: a, rightMatrix: b, resultMatrix: c)
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
let p95 = samples[min(samples.count - 1, Int(Double(samples.count - 1) * 0.95))]
let flops = 2.0 * Double(m) * Double(n) * Double(k)
let tflops = flops / median / 1.0e12
let readWriteBytes = Double(m * k + k * n + m * n) * Double(elementBytes)
let bandwidthGBs = readWriteBytes / median / 1.0e9

print("mps_matrix_probe")
print("device: \(device.name)")
print("shape_m_n_k: \(m) \(n) \(k)")
print("dtype: fp16_fp16_to_fp16")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", tflops))
print(String(format: "stream_bytes_gb: %.6f", readWriteBytes / 1.0e9))
print(String(format: "stream_bandwidth_gb_s: %.3f", bandwidthGBs))
