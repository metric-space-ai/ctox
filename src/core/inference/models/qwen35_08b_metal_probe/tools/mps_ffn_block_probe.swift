import Foundation
import Metal
import MetalPerformanceShaders

func argInt(_ index: Int, _ fallback: Int) -> Int {
    guard CommandLine.arguments.count > index, let value = Int(CommandLine.arguments[index]) else {
        return fallback
    }
    return value
}

let tokens = argInt(1, 4096)
let hidden = argInt(2, 1024)
let intermediate = argInt(3, 3584)
let iterations = argInt(4, 5)
let warmup = argInt(5, 2)

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
let xRowBytes = alignedRowBytes(hidden, elementBytes)
let guWeightRowBytes = alignedRowBytes(intermediate * 2, elementBytes)
let guRowBytes = alignedRowBytes(intermediate * 2, elementBytes)
let actRowBytes = alignedRowBytes(intermediate, elementBytes)
let downWeightRowBytes = alignedRowBytes(hidden, elementBytes)
let outRowBytes = alignedRowBytes(hidden, elementBytes)

let xBytes = tokens * xRowBytes
let guWeightBytes = hidden * guWeightRowBytes
let guBytes = tokens * guRowBytes
let actBytes = tokens * actRowBytes
let downWeightBytes = intermediate * downWeightRowBytes
let outBytes = tokens * outRowBytes

func makeBuffer(_ length: Int) -> MTLBuffer {
    guard let buffer = device.makeBuffer(length: length, options: .storageModeShared) else {
        fatalError("failed to allocate \(length) bytes")
    }
    return buffer
}

let xBuffer = makeBuffer(xBytes)
let guWeightBuffer = makeBuffer(guWeightBytes)
let guBuffer = makeBuffer(guBytes)
let actBuffer = makeBuffer(actBytes)
let downWeightBuffer = makeBuffer(downWeightBytes)
let outBuffer = makeBuffer(outBytes)

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
fillHalfBuffer(guWeightBuffer, byteCount: guWeightBytes, seed: 0x9abcdef0)
fillHalfBuffer(downWeightBuffer, byteCount: downWeightBytes, seed: 0x0badcafe)

let xDesc = MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: xRowBytes, dataType: .float16)
let guWeightDesc = MPSMatrixDescriptor(rows: hidden, columns: intermediate * 2, rowBytes: guWeightRowBytes, dataType: .float16)
let guDesc = MPSMatrixDescriptor(rows: tokens, columns: intermediate * 2, rowBytes: guRowBytes, dataType: .float16)
let actDesc = MPSMatrixDescriptor(rows: tokens, columns: intermediate, rowBytes: actRowBytes, dataType: .float16)
let downWeightDesc = MPSMatrixDescriptor(rows: intermediate, columns: hidden, rowBytes: downWeightRowBytes, dataType: .float16)
let outDesc = MPSMatrixDescriptor(rows: tokens, columns: hidden, rowBytes: outRowBytes, dataType: .float16)

let x = MPSMatrix(buffer: xBuffer, descriptor: xDesc)
let guWeight = MPSMatrix(buffer: guWeightBuffer, descriptor: guWeightDesc)
let gu = MPSMatrix(buffer: guBuffer, descriptor: guDesc)
let act = MPSMatrix(buffer: actBuffer, descriptor: actDesc)
let downWeight = MPSMatrix(buffer: downWeightBuffer, descriptor: downWeightDesc)
let out = MPSMatrix(buffer: outBuffer, descriptor: outDesc)

let gateUpGemm = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: tokens,
    resultColumns: intermediate * 2,
    interiorColumns: hidden,
    alpha: 1.0,
    beta: 0.0
)

let downGemm = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: tokens,
    resultColumns: hidden,
    interiorColumns: intermediate,
    alpha: 1.0,
    beta: 0.0
)

let source = """
#include <metal_stdlib>
using namespace metal;

kernel void swiglu_kernel(
    device const half* gate_up [[buffer(0)]],
    device half* act [[buffer(1)]],
    constant uint& intermediate [[buffer(2)]],
    constant uint& gu_stride [[buffer(3)]],
    constant uint& act_stride [[buffer(4)]],
    uint2 gid [[thread_position_in_grid]]
) {
    const uint token = gid.y;
    const uint col = gid.x;
    if (col >= intermediate) {
        return;
    }
    const uint gu_base = token * gu_stride;
    const uint act_base = token * act_stride;
    const float gate = float(gate_up[gu_base + col]);
    const float up = float(gate_up[gu_base + intermediate + col]);
    const float sigmoid = 1.0f / (1.0f + exp(-gate));
    act[act_base + col] = half((gate * sigmoid) * up);
}
"""

let library = try device.makeLibrary(source: source, options: nil)
guard let function = library.makeFunction(name: "swiglu_kernel") else {
    fatalError("missing swiglu_kernel")
}
let swiglu = try device.makeComputePipelineState(function: function)

var intermediateU32 = UInt32(intermediate)
var guStrideU32 = UInt32(guRowBytes / elementBytes)
var actStrideU32 = UInt32(actRowBytes / elementBytes)

func encodeSwiglu(_ commandBuffer: MTLCommandBuffer) {
    guard let encoder = commandBuffer.makeComputeCommandEncoder() else {
        fatalError("failed to make compute encoder")
    }
    encoder.setComputePipelineState(swiglu)
    encoder.setBuffer(guBuffer, offset: 0, index: 0)
    encoder.setBuffer(actBuffer, offset: 0, index: 1)
    encoder.setBytes(&intermediateU32, length: MemoryLayout<UInt32>.stride, index: 2)
    encoder.setBytes(&guStrideU32, length: MemoryLayout<UInt32>.stride, index: 3)
    encoder.setBytes(&actStrideU32, length: MemoryLayout<UInt32>.stride, index: 4)
    let tg = MTLSize(width: 256, height: 1, depth: 1)
    let grid = MTLSize(width: intermediate, height: tokens, depth: 1)
    encoder.dispatchThreads(grid, threadsPerThreadgroup: tg)
    encoder.endEncoding()
}

func runOnce() -> Double {
    guard let commandBuffer = queue.makeCommandBuffer() else {
        fatalError("failed to create command buffer")
    }
    let start = DispatchTime.now().uptimeNanoseconds
    gateUpGemm.encode(commandBuffer: commandBuffer, leftMatrix: x, rightMatrix: guWeight, resultMatrix: gu)
    encodeSwiglu(commandBuffer)
    downGemm.encode(commandBuffer: commandBuffer, leftMatrix: act, rightMatrix: downWeight, resultMatrix: out)
    commandBuffer.commit()
    commandBuffer.waitUntilCompleted()
    if let error = commandBuffer.error {
        fatalError("command buffer failed: \(error)")
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
let ffnFlops = 2.0 * Double(tokens) * Double(hidden) * Double(intermediate * 2)
    + 2.0 * Double(tokens) * Double(intermediate) * Double(hidden)
let visibleBytes = Double(xBytes + guWeightBytes + guBytes + actBytes + downWeightBytes + outBytes)

let outPtr = outBuffer.contents().bindMemory(to: UInt16.self, capacity: outBytes / elementBytes)
var checksum: UInt64 = 0
for i in 0..<min(16, outBytes / elementBytes) {
    checksum &+= UInt64(outPtr[i])
}

print("mps_ffn_block_probe")
print("device: \(device.name)")
print("shape: tokens=\(tokens) hidden=\(hidden) intermediate=\(intermediate)")
print("backend: MPSMatrixMultiplication + MSL SwiGLU")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops: %.3f", ffnFlops / median / 1.0e12))
print(String(format: "visible_gb_s: %.3f", visibleBytes / median / 1.0e9))
print("checksum16: \(checksum)")
