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

let tokens = argInt(1, 4096)
let hidden = argInt(2, 1024)
let qkvRows = argInt(3, 6144)
let zRows = argInt(4, 2048)
let iterations = argInt(5, 3)
let warmup = argInt(6, 1)
let combinedRows = qkvRows + zRows

guard let device = MTLCreateSystemDefaultDevice(),
      MPSSupportsMTLDevice(device),
      let queue = device.makeCommandQueue() else {
    fatalError("failed to initialize Metal/MPS")
}

let elementBytes = MemoryLayout<UInt16>.stride
let xRowBytes = alignedRowBytes(hidden, elementBytes)
let wRowBytes = alignedRowBytes(combinedRows, elementBytes)
let yRowBytes = alignedRowBytes(combinedRows, elementBytes)
let xBytes = tokens * xRowBytes
let wBytes = hidden * wRowBytes
let yBytes = tokens * yRowBytes

func makeBuffer(_ length: Int) -> MTLBuffer {
    guard let buffer = device.makeBuffer(length: length, options: .storageModeShared) else {
        fatalError("failed to allocate \(length) bytes")
    }
    return buffer
}

let xBuffer = makeBuffer(xBytes)
let wBuffer = makeBuffer(wBytes)
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
fillHalfBuffer(wBuffer, byteCount: wBytes, seed: 0x9abcdef0)

let x = MPSMatrix(buffer: xBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: xRowBytes, dataType: .float16))
let w = MPSMatrix(buffer: wBuffer, descriptor: MPSMatrixDescriptor(rows: hidden, columns: combinedRows, rowBytes: wRowBytes, dataType: .float16))
let y = MPSMatrix(buffer: yBuffer, descriptor: MPSMatrixDescriptor(rows: tokens, columns: combinedRows, rowBytes: yRowBytes, dataType: .float16))

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
let visibleBytes = Double(xBytes + wBytes + yBytes)
let yPtr = yBuffer.contents().bindMemory(to: UInt16.self, capacity: yBytes / elementBytes)
var checksum: UInt64 = 0
for i in 0..<min(16, yBytes / elementBytes) { checksum &+= UInt64(yPtr[i]) }

print("mps_deltanet_project_probe")
print("device: \(device.name)")
print("shape: tokens=\(tokens) hidden=\(hidden) qkv_rows=\(qkvRows) z_rows=\(zRows) combined_rows=\(combinedRows)")
print("backend: MPSMatrix synthetic combined QKV+Z")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", flops / median / 1.0e12))
print(String(format: "visible_gb_s: %.3f", visibleBytes / median / 1.0e9))
print("checksum16: \(checksum)")
