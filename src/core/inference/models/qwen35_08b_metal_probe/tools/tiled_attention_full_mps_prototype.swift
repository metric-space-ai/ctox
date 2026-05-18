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
        ptr[i] = UInt16(0x3400) | mantissa
    }
}

func percentile(_ sorted: [Double], _ q: Double) -> Double {
    if sorted.isEmpty { return 0.0 }
    let idx = min(sorted.count - 1, Int(Double(sorted.count - 1) * q + 0.5))
    return sorted[idx]
}

let shaderSource = #"""
#include <metal_stdlib>
using namespace metal;

kernel void tiled_attn_init_rows(
    device float* m_state [[buffer(0)]],
    device float* l_state [[buffer(1)]],
    device half* out [[buffer(2)]],
    constant uint& q_tile [[buffer(3)]],
    constant uint& head_dim [[buffer(4)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_tile * head_dim;
    if (gid < q_tile) {
        m_state[gid] = -INFINITY;
        l_state[gid] = 0.0f;
    }
    if (gid < total) {
        out[gid] = half(0.0f);
    }
}

kernel void tiled_attn_softmax_update(
    device half* score [[buffer(0)]],
    device half* prob [[buffer(1)]],
    device float* m_state [[buffer(2)]],
    device float* l_state [[buffer(3)]],
    device float* old_scale [[buffer(4)]],
    device float* inv_l [[buffer(5)]],
    device float* pv_scale [[buffer(6)]],
    constant uint& q_tile [[buffer(7)]],
    constant uint& k_tile [[buffer(8)]],
    constant uint& score_row_stride [[buffer(9)]],
    constant uint& q_block [[buffer(10)]],
    constant uint& k_block [[buffer(11)]],
    uint row [[thread_position_in_grid]]
) {
    if (row >= q_tile) {
        return;
    }

    const uint q_abs = q_block * q_tile + row;
    float tile_m = -INFINITY;
    for (uint col = 0; col < k_tile; ++col) {
        const uint k_abs = k_block * k_tile + col;
        float s = (k_abs <= q_abs) ? float(score[row * score_row_stride + col]) : -INFINITY;
        tile_m = max(tile_m, s);
    }

    float tile_l = 0.0f;
    for (uint col = 0; col < k_tile; ++col) {
        const uint k_abs = k_block * k_tile + col;
        float p = 0.0f;
        if (k_abs <= q_abs && isfinite(tile_m)) {
            p = exp(float(score[row * score_row_stride + col]) - tile_m);
        }
        tile_l += p;
        prob[row * score_row_stride + col] = half(clamp(p, 0.0f, 65504.0f));
    }

    const float prev_m = m_state[row];
    const float prev_l = l_state[row];
    const float next_m = max(prev_m, tile_m);
    const float prev_scale = isfinite(prev_m) ? prev_l * exp(prev_m - next_m) : 0.0f;
    const float tile_scale = isfinite(tile_m) ? tile_l * exp(tile_m - next_m) : 0.0f;
    const float next_l = prev_scale + tile_scale;

    old_scale[row] = prev_scale;
    pv_scale[row] = isfinite(tile_m) ? exp(tile_m - next_m) : 0.0f;
    inv_l[row] = (next_l > 0.0f) ? (1.0f / next_l) : 0.0f;
    m_state[row] = next_m;
    l_state[row] = next_l;
}

kernel void tiled_attn_softmax_update_simd32(
    device half* score [[buffer(0)]],
    device half* prob [[buffer(1)]],
    device float* m_state [[buffer(2)]],
    device float* l_state [[buffer(3)]],
    device float* old_scale [[buffer(4)]],
    device float* inv_l [[buffer(5)]],
    device float* pv_scale [[buffer(6)]],
    constant uint& q_tile [[buffer(7)]],
    constant uint& k_tile [[buffer(8)]],
    constant uint& score_row_stride [[buffer(9)]],
    constant uint& q_block [[buffer(10)]],
    constant uint& k_block [[buffer(11)]],
    constant uint& query_tile [[buffer(12)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint lane = gid & 31u;
    const uint row = gid >> 5u;
    if (row >= q_tile) {
        return;
    }

    const uint query_row = row % query_tile;
    const uint q_abs = q_block * query_tile + query_row;
    float local_m = -INFINITY;
    for (uint col = lane; col < k_tile; col += 32u) {
        const uint k_abs = k_block * k_tile + col;
        const float s = (k_abs <= q_abs) ? float(score[row * score_row_stride + col]) : -INFINITY;
        local_m = max(local_m, s);
    }
    const float tile_m = simd_max(local_m);

    float local_l = 0.0f;
    for (uint col = lane; col < k_tile; col += 32u) {
        const uint k_abs = k_block * k_tile + col;
        float p = 0.0f;
        if (k_abs <= q_abs && isfinite(tile_m)) {
            p = exp(float(score[row * score_row_stride + col]) - tile_m);
        }
        local_l += p;
        prob[row * score_row_stride + col] = half(clamp(p, 0.0f, 65504.0f));
    }
    const float tile_l = simd_sum(local_l);

    if (lane == 0u) {
        const float prev_m = m_state[row];
        const float prev_l = l_state[row];
        const float next_m = max(prev_m, tile_m);
        const float prev_scale = isfinite(prev_m) ? prev_l * exp(prev_m - next_m) : 0.0f;
        const float tile_scale = isfinite(tile_m) ? tile_l * exp(tile_m - next_m) : 0.0f;
        const float next_l = prev_scale + tile_scale;

        old_scale[row] = prev_scale;
        pv_scale[row] = isfinite(tile_m) ? exp(tile_m - next_m) : 0.0f;
        inv_l[row] = (next_l > 0.0f) ? (1.0f / next_l) : 0.0f;
        m_state[row] = next_m;
        l_state[row] = next_l;
    }
}

kernel void tiled_attn_combine(
    device half* out [[buffer(0)]],
    device half* pv [[buffer(1)]],
    device float* old_scale [[buffer(2)]],
    device float* inv_l [[buffer(3)]],
    device float* pv_scale [[buffer(4)]],
    constant uint& q_tile [[buffer(5)]],
    constant uint& head_dim [[buffer(6)]],
    constant uint& out_row_stride [[buffer(7)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_tile * head_dim;
    if (gid >= total) {
        return;
    }
    const uint row = gid / head_dim;
    const uint col = gid - row * head_dim;
    const uint offset = row * out_row_stride + col;
    const float value = (float(out[offset]) * old_scale[row] + float(pv[offset]) * pv_scale[row]) * inv_l[row];
    out[offset] = half(clamp(value, -65504.0f, 65504.0f));
}

kernel void tiled_attn_store_global(
    device const half* out_tile [[buffer(0)]],
    device half* global_out [[buffer(1)]],
    constant uint& q_rows [[buffer(2)]],
    constant uint& head_dim [[buffer(3)]],
    constant uint& out_row_stride [[buffer(4)]],
    constant uint& global_row_stride [[buffer(5)]],
    constant uint& q_block [[buffer(6)]],
    constant uint& q_tile [[buffer(7)]],
    constant uint& tokens [[buffer(8)]],
    uint gid [[thread_position_in_grid]]
) {
    const uint total = q_rows * head_dim;
    if (gid >= total) {
        return;
    }
    const uint row = gid / head_dim;
    const uint col = gid - row * head_dim;
    const uint query_row = row % q_tile;
    const uint q_abs = q_block * q_tile + query_row;
    if (q_abs >= tokens) {
        return;
    }
    const uint global_row = q_block * q_rows + row;
    global_out[global_row * global_row_stride + col] = out_tile[row * out_row_stride + col];
}
"""#

let tokens = argInt(1, 4096)
let qTile = argInt(2, 256)
let kTile = argInt(3, 1024)
let iterations = argInt(4, 3)
let warmup = argInt(5, 1)
let headsPerGroup = argInt(6, 4)
let useMatrixOrigins = argInt(7, 1) != 0
let qualityCheck = argInt(8, 0) != 0
let headDim = 256

guard tokens > 0, qTile > 0, kTile > 0, iterations > 0, headsPerGroup > 0 else {
    fatalError("tokens, qTile, kTile, iterations, and headsPerGroup must be > 0")
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

let library = try device.makeLibrary(source: shaderSource, options: nil)
func pipeline(_ name: String) throws -> MTLComputePipelineState {
    guard let fn = library.makeFunction(name: name) else {
        throw NSError(domain: "ctox", code: 1, userInfo: [NSLocalizedDescriptionKey: "missing \(name)"])
    }
    return try device.makeComputePipelineState(function: fn)
}
let initPSO = try pipeline("tiled_attn_init_rows")
let softmaxPSO = try pipeline("tiled_attn_softmax_update_simd32")
let combinePSO = try pipeline("tiled_attn_combine")
let storePSO = try pipeline("tiled_attn_store_global")

let elementBytes = MemoryLayout<UInt16>.stride
let qRowBytes = alignedRowBytes(headDim, elementBytes)
let kRowBytes = alignedRowBytes(kTile, elementBytes)
let vRowBytes = alignedRowBytes(headDim, elementBytes)
let scoreRowBytes = alignedRowBytes(kTile, elementBytes)
let outRowBytes = alignedRowBytes(headDim, elementBytes)
let qRows = qTile * headsPerGroup
let qMatrixRows = useMatrixOrigins ? tokens * headsPerGroup : qRows
let kMatrixColumns = useMatrixOrigins ? tokens : kTile
let vMatrixRows = useMatrixOrigins ? tokens : kTile
let qBytes = qMatrixRows * qRowBytes
let kBytes = headDim * alignedRowBytes(kMatrixColumns, elementBytes)
let vBytes = vMatrixRows * vRowBytes
let scoreBytes = qRows * scoreRowBytes
let outBytes = qRows * outRowBytes
let globalOutBytes = qMatrixRows * outRowBytes
let rowStateBytes = qRows * MemoryLayout<Float>.stride

func makeBuffer(_ bytes: Int, _ options: MTLResourceOptions = .storageModeShared) -> MTLBuffer {
    guard let buffer = device.makeBuffer(length: bytes, options: options) else {
        fatalError("failed to allocate \(bytes) bytes")
    }
    return buffer
}

let qBuffer = makeBuffer(qBytes)
let kBuffer = makeBuffer(kBytes)
let vBuffer = makeBuffer(vBytes)
let scoreBuffer = makeBuffer(scoreBytes)
let probBuffer = makeBuffer(scoreBytes)
let pvBuffer = makeBuffer(outBytes)
let outBuffer = makeBuffer(outBytes)
let globalOutBuffer = makeBuffer(globalOutBytes)
let mState = makeBuffer(rowStateBytes)
let lState = makeBuffer(rowStateBytes)
let oldScale = makeBuffer(rowStateBytes)
let invL = makeBuffer(rowStateBytes)
let pvScale = makeBuffer(rowStateBytes)
fillHalfBuffer(qBuffer, byteCount: qBytes, seed: 0x12345678)
fillHalfBuffer(kBuffer, byteCount: kBytes, seed: 0x9abcdef0)
fillHalfBuffer(vBuffer, byteCount: vBytes, seed: 0x0badcafe)

let kMatrixRowBytes = alignedRowBytes(kMatrixColumns, elementBytes)
let qDesc = MPSMatrixDescriptor(rows: qMatrixRows, columns: headDim, rowBytes: qRowBytes, dataType: .float16)
let kDesc = MPSMatrixDescriptor(rows: headDim, columns: kMatrixColumns, rowBytes: kMatrixRowBytes, dataType: .float16)
let scoreDesc = MPSMatrixDescriptor(rows: qRows, columns: kTile, rowBytes: scoreRowBytes, dataType: .float16)
let vDesc = MPSMatrixDescriptor(rows: vMatrixRows, columns: headDim, rowBytes: vRowBytes, dataType: .float16)
let outDesc = MPSMatrixDescriptor(rows: qRows, columns: headDim, rowBytes: outRowBytes, dataType: .float16)
let q = MPSMatrix(buffer: qBuffer, descriptor: qDesc)
let k = MPSMatrix(buffer: kBuffer, descriptor: kDesc)
let score = MPSMatrix(buffer: scoreBuffer, descriptor: scoreDesc)
let prob = MPSMatrix(buffer: probBuffer, descriptor: scoreDesc)
let v = MPSMatrix(buffer: vBuffer, descriptor: vDesc)
let pv = MPSMatrix(buffer: pvBuffer, descriptor: outDesc)

let qBlocks = (tokens + qTile - 1) / qTile
let kBlocks = (tokens + kTile - 1) / kTile
var causalTilePairs = 0
for qb in 0..<qBlocks {
    let qLast = min((qb + 1) * qTile, tokens) - 1
    causalTilePairs += min(kBlocks, qLast / kTile + 1)
}

let qk = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: qRows,
    resultColumns: kTile,
    interiorColumns: headDim,
    alpha: 1.0 / sqrt(Double(headDim)),
    beta: 0.0
)
let pvMul = MPSMatrixMultiplication(
    device: device,
    transposeLeft: false,
    transposeRight: false,
    resultRows: qRows,
    resultColumns: headDim,
    interiorColumns: kTile,
    alpha: 1.0,
    beta: 0.0
)

func encodeInit(_ commandBuffer: MTLCommandBuffer) {
    guard let encoder = commandBuffer.makeComputeCommandEncoder() else {
        fatalError("failed to create init encoder")
    }
    var qt = UInt32(qRows)
    var hd = UInt32(headDim)
    encoder.setComputePipelineState(initPSO)
    encoder.setBuffer(mState, offset: 0, index: 0)
    encoder.setBuffer(lState, offset: 0, index: 1)
    encoder.setBuffer(outBuffer, offset: 0, index: 2)
    encoder.setBytes(&qt, length: MemoryLayout<UInt32>.stride, index: 3)
    encoder.setBytes(&hd, length: MemoryLayout<UInt32>.stride, index: 4)
    let total = max(qRows, qRows * headDim)
    encoder.dispatchThreads(MTLSize(width: total, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: 256, height: 1, depth: 1))
    encoder.endEncoding()
}

func encodeSoftmax(_ commandBuffer: MTLCommandBuffer, qBlock: Int, kBlock: Int) {
    guard let encoder = commandBuffer.makeComputeCommandEncoder() else {
        fatalError("failed to create softmax encoder")
    }
    var qt = UInt32(qRows)
    var kt = UInt32(kTile)
    var stride = UInt32(scoreRowBytes / elementBytes)
    var qb = UInt32(qBlock)
    var kb = UInt32(kBlock)
    var queryTile = UInt32(qTile)
    encoder.setComputePipelineState(softmaxPSO)
    encoder.setBuffer(scoreBuffer, offset: 0, index: 0)
    encoder.setBuffer(probBuffer, offset: 0, index: 1)
    encoder.setBuffer(mState, offset: 0, index: 2)
    encoder.setBuffer(lState, offset: 0, index: 3)
    encoder.setBuffer(oldScale, offset: 0, index: 4)
    encoder.setBuffer(invL, offset: 0, index: 5)
    encoder.setBuffer(pvScale, offset: 0, index: 6)
    encoder.setBytes(&qt, length: MemoryLayout<UInt32>.stride, index: 7)
    encoder.setBytes(&kt, length: MemoryLayout<UInt32>.stride, index: 8)
    encoder.setBytes(&stride, length: MemoryLayout<UInt32>.stride, index: 9)
    encoder.setBytes(&qb, length: MemoryLayout<UInt32>.stride, index: 10)
    encoder.setBytes(&kb, length: MemoryLayout<UInt32>.stride, index: 11)
    encoder.setBytes(&queryTile, length: MemoryLayout<UInt32>.stride, index: 12)
    encoder.dispatchThreads(MTLSize(width: qRows * 32, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: 256, height: 1, depth: 1))
    encoder.endEncoding()
}

func encodeCombine(_ commandBuffer: MTLCommandBuffer) {
    guard let encoder = commandBuffer.makeComputeCommandEncoder() else {
        fatalError("failed to create combine encoder")
    }
    var qt = UInt32(qRows)
    var hd = UInt32(headDim)
    var stride = UInt32(outRowBytes / elementBytes)
    encoder.setComputePipelineState(combinePSO)
    encoder.setBuffer(outBuffer, offset: 0, index: 0)
    encoder.setBuffer(pvBuffer, offset: 0, index: 1)
    encoder.setBuffer(oldScale, offset: 0, index: 2)
    encoder.setBuffer(invL, offset: 0, index: 3)
    encoder.setBuffer(pvScale, offset: 0, index: 4)
    encoder.setBytes(&qt, length: MemoryLayout<UInt32>.stride, index: 5)
    encoder.setBytes(&hd, length: MemoryLayout<UInt32>.stride, index: 6)
    encoder.setBytes(&stride, length: MemoryLayout<UInt32>.stride, index: 7)
    encoder.dispatchThreads(MTLSize(width: qRows * headDim, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: 256, height: 1, depth: 1))
    encoder.endEncoding()
}

func encodeStore(_ commandBuffer: MTLCommandBuffer, qBlock: Int) {
    guard let encoder = commandBuffer.makeComputeCommandEncoder() else {
        fatalError("failed to create store encoder")
    }
    var rows = UInt32(qRows)
    var hd = UInt32(headDim)
    var outStride = UInt32(outRowBytes / elementBytes)
    var globalStride = UInt32(outRowBytes / elementBytes)
    var qb = UInt32(qBlock)
    var queryTile = UInt32(qTile)
    var tokenCount = UInt32(tokens)
    encoder.setComputePipelineState(storePSO)
    encoder.setBuffer(outBuffer, offset: 0, index: 0)
    encoder.setBuffer(globalOutBuffer, offset: 0, index: 1)
    encoder.setBytes(&rows, length: MemoryLayout<UInt32>.stride, index: 2)
    encoder.setBytes(&hd, length: MemoryLayout<UInt32>.stride, index: 3)
    encoder.setBytes(&outStride, length: MemoryLayout<UInt32>.stride, index: 4)
    encoder.setBytes(&globalStride, length: MemoryLayout<UInt32>.stride, index: 5)
    encoder.setBytes(&qb, length: MemoryLayout<UInt32>.stride, index: 6)
    encoder.setBytes(&queryTile, length: MemoryLayout<UInt32>.stride, index: 7)
    encoder.setBytes(&tokenCount, length: MemoryLayout<UInt32>.stride, index: 8)
    encoder.dispatchThreads(MTLSize(width: qRows * headDim, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: 256, height: 1, depth: 1))
    encoder.endEncoding()
}

func runOnce() -> Double {
    guard let commandBuffer = queue.makeCommandBuffer() else {
        fatalError("failed to create command buffer")
    }
    let start = DispatchTime.now().uptimeNanoseconds
    for qb in 0..<qBlocks {
        encodeInit(commandBuffer)
        let qLast = min((qb + 1) * qTile, tokens) - 1
        let allowedKBlocks = min(kBlocks, qLast / kTile + 1)
        for kb in 0..<allowedKBlocks {
            if useMatrixOrigins {
                qk.leftMatrixOrigin = MTLOrigin(x: qb * qRows, y: 0, z: 0)
                qk.rightMatrixOrigin = MTLOrigin(x: 0, y: kb * kTile, z: 0)
                qk.resultMatrixOrigin = MTLOrigin(x: 0, y: 0, z: 0)
            }
            qk.encode(commandBuffer: commandBuffer, leftMatrix: q, rightMatrix: k, resultMatrix: score)
            encodeSoftmax(commandBuffer, qBlock: qb, kBlock: kb)
            if useMatrixOrigins {
                pvMul.leftMatrixOrigin = MTLOrigin(x: 0, y: 0, z: 0)
                pvMul.rightMatrixOrigin = MTLOrigin(x: kb * kTile, y: 0, z: 0)
                pvMul.resultMatrixOrigin = MTLOrigin(x: 0, y: 0, z: 0)
            }
            pvMul.encode(commandBuffer: commandBuffer, leftMatrix: prob, rightMatrix: v, resultMatrix: pv)
            encodeCombine(commandBuffer)
        }
        encodeStore(commandBuffer, qBlock: qb)
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
let qkFlops = 2.0 * Double(qRows) * Double(kTile) * Double(headDim) * Double(causalTilePairs)
let pvFlops = qkFlops
let tflops = (qkFlops + pvFlops) / median / 1.0e12
let qTileBytes = qRows * qRowBytes
let kTileBytes = headDim * kRowBytes
let vTileBytes = kTile * vRowBytes
let qkBytesPerPair = qTileBytes + kTileBytes + scoreBytes
let pvBytesPerPair = scoreBytes + vTileBytes + outBytes
let modeledTrafficBytes = Double(causalTilePairs * (qkBytesPerPair + pvBytesPerPair))

func halfToFloat(_ bits: UInt16) -> Float {
    Float(Float16(bitPattern: bits))
}

func sparseIndices(_ count: Int) -> [Int] {
    if count <= 0 {
        return []
    }
    var values = [
        0,
        count / 11,
        count / 7,
        count / 3,
        count / 2,
        count * 2 / 3,
        count * 6 / 7,
        count - 1,
    ]
    values.sort()
    var deduped: [Int] = []
    for value in values {
        if deduped.last != value {
            deduped.append(value)
        }
    }
    return deduped
}

func runQualityCheck() {
    let qPtr = qBuffer.contents().bindMemory(to: UInt16.self, capacity: qBytes / 2)
    let kPtr = kBuffer.contents().bindMemory(to: UInt16.self, capacity: kBytes / 2)
    let vPtr = vBuffer.contents().bindMemory(to: UInt16.self, capacity: vBytes / 2)
    let outPtr = globalOutBuffer.contents().bindMemory(to: UInt16.self, capacity: globalOutBytes / 2)
    let qStride = qRowBytes / elementBytes
    let kStride = kMatrixRowBytes / elementBytes
    let vStride = vRowBytes / elementBytes
    let outStride = outRowBytes / elementBytes
    let scale = Float(1.0 / sqrt(Double(headDim)))
    let rowSamples = sparseIndices(qMatrixRows)
    let dimSamples = sparseIndices(headDim)
    var checked = 0
    var maxAbs: Float = 0.0
    var sumAbs: Double = 0.0
    var worstRow = 0
    var worstDim = 0
    var worstRef: Float = 0.0
    var worstGpu: Float = 0.0
    var scores = Array(repeating: Float(0), count: tokens)

    for globalRow in rowSamples {
        let qBlock = globalRow / qRows
        let localRow = globalRow - qBlock * qRows
        let queryRow = localRow % qTile
        let qAbs = qBlock * qTile + queryRow
        if qAbs >= tokens {
            continue
        }
        var maxScore = -Float.infinity
        for key in 0...qAbs {
            var dot: Float = 0.0
            for dim in 0..<headDim {
                dot += halfToFloat(qPtr[globalRow * qStride + dim]) * halfToFloat(kPtr[dim * kStride + key])
            }
            let score = dot * scale
            scores[key] = score
            if score > maxScore {
                maxScore = score
            }
        }
        var denom: Float = 0.0
        for key in 0...qAbs {
            let p = exp(scores[key] - maxScore)
            scores[key] = p
            denom += p
        }
        for dim in dimSamples {
            var acc: Float = 0.0
            for key in 0...qAbs {
                acc += scores[key] * halfToFloat(vPtr[key * vStride + dim])
            }
            let ref = acc / denom
            let gpu = halfToFloat(outPtr[globalRow * outStride + dim])
            let absErr = abs(ref - gpu)
            checked += 1
            sumAbs += Double(absErr)
            if absErr > maxAbs {
                maxAbs = absErr
                worstRow = globalRow
                worstDim = dim
                worstRef = ref
                worstGpu = gpu
            }
        }
    }

    let meanAbs = checked > 0 ? sumAbs / Double(checked) : 0.0
    print("quality_checked_points: \(checked)")
    print(String(format: "quality_mean_abs_error: %.9f", meanAbs))
    print(String(format: "quality_max_abs_error: %.9f", maxAbs))
    print("quality_worst_row: \(worstRow)")
    print("quality_worst_dim: \(worstDim)")
    print(String(format: "quality_worst_ref: %.9f", worstRef))
    print(String(format: "quality_worst_gpu: %.9f", worstGpu))
}

print("tiled_attention_full_mps_prototype")
print("device: \(device.name)")
print("tokens: \(tokens)")
print("q_tile: \(qTile)")
print("k_tile: \(kTile)")
print("heads_per_group: \(headsPerGroup)")
print("q_rows_per_tile: \(qRows)")
print("matrix_origins: \(useMatrixOrigins)")
print("quality_check: \(qualityCheck)")
print("head_dim: \(headDim)")
print("q_blocks: \(qBlocks)")
print("k_blocks: \(kBlocks)")
print("causal_tile_pairs: \(causalTilePairs)")
print("iterations: \(iterations)")
print("warmup: \(warmup)")
print("contract: synthetic tiled QK + block softmax + PV + online combine; matrix origins emulate real Q/K/V tile slicing")
print(String(format: "score_tile_mib: %.3f", Double(scoreBytes) / (1024.0 * 1024.0)))
print(String(format: "out_tile_mib: %.3f", Double(outBytes) / (1024.0 * 1024.0)))
print(String(format: "modeled_tile_traffic_mib: %.3f", modeledTrafficBytes / (1024.0 * 1024.0)))
print(String(format: "median_s: %.9f", median))
print(String(format: "p95_s: %.9f", p95))
print(String(format: "effective_tflops_qk_plus_pv: %.3f", tflops))
print(String(format: "effective_gb_s_modeled_tile_traffic: %.3f", modeledTrafficBytes / median / 1.0e9))
print(String(format: "tile_pairs_per_s: %.3f", Double(causalTilePairs) / median))
if qualityCheck {
    runQualityCheck()
}
