@import Foundation;
@import Metal;
@import MetalPerformanceShaders;

#include <stdint.h>

struct CtoxMpsFfnPlan {
    MPSMatrixMultiplication *gate_up;
    MPSMatrixMultiplication *down;
    NSUInteger tokens;
    NSUInteger hidden;
    NSUInteger intermediate;
    NSUInteger x_row_bytes;
    NSUInteger gate_up_weight_row_bytes;
    NSUInteger gate_up_row_bytes;
    NSUInteger act_row_bytes;
    NSUInteger down_weight_row_bytes;
    NSUInteger out_row_bytes;
};

struct CtoxMpsDeltaProjectPlan {
    MPSMatrixMultiplication *project;
    NSUInteger tokens;
    NSUInteger hidden;
    NSUInteger qkvz_rows;
    NSUInteger x_row_bytes;
    NSUInteger weight_row_bytes;
    NSUInteger out_row_bytes;
};

struct CtoxMpsTiledAttentionPlan {
    MPSMatrixMultiplication *qk;
    MPSMatrixMultiplication *pv;
    NSUInteger tokens;
    NSUInteger q_tile;
    NSUInteger k_tile;
    NSUInteger head_dim;
    NSUInteger heads_per_group;
    NSUInteger q_rows;
    NSUInteger q_row_bytes;
    NSUInteger k_row_bytes;
    NSUInteger v_row_bytes;
    NSUInteger score_row_bytes;
    NSUInteger out_row_bytes;
};

extern "C" int ctox_mps_device_supports(void *device_ptr) {
    @autoreleasepool {
        id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
        return MPSSupportsMTLDevice(device) ? 1 : 0;
    }
}

extern "C" void *ctox_mps_delta_project_plan_new(
    void *device_ptr,
    uint32_t tokens,
    uint32_t hidden,
    uint32_t qkvz_rows,
    uint32_t x_row_bytes,
    uint32_t weight_row_bytes,
    uint32_t out_row_bytes
) {
    @autoreleasepool {
        id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
        if (!MPSSupportsMTLDevice(device)) {
            return nullptr;
        }
        CtoxMpsDeltaProjectPlan *plan = new CtoxMpsDeltaProjectPlan();
        plan->tokens = tokens;
        plan->hidden = hidden;
        plan->qkvz_rows = qkvz_rows;
        plan->x_row_bytes = x_row_bytes;
        plan->weight_row_bytes = weight_row_bytes;
        plan->out_row_bytes = out_row_bytes;
        plan->project = [[MPSMatrixMultiplication alloc]
            initWithDevice:device
             transposeLeft:NO
            transposeRight:NO
                resultRows:tokens
             resultColumns:qkvz_rows
           interiorColumns:hidden
                     alpha:1.0
                      beta:0.0];
        return plan;
    }
}

extern "C" void ctox_mps_delta_project_plan_free(void *plan_ptr) {
    CtoxMpsDeltaProjectPlan *plan = static_cast<CtoxMpsDeltaProjectPlan *>(plan_ptr);
    delete plan;
}

extern "C" void *ctox_mps_tiled_attention_plan_new(
    void *device_ptr,
    uint32_t tokens,
    uint32_t q_tile,
    uint32_t k_tile,
    uint32_t head_dim,
    uint32_t heads_per_group,
    uint32_t q_row_bytes,
    uint32_t k_row_bytes,
    uint32_t v_row_bytes,
    uint32_t score_row_bytes,
    uint32_t out_row_bytes
) {
    @autoreleasepool {
        id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
        if (!MPSSupportsMTLDevice(device)) {
            return nullptr;
        }
        CtoxMpsTiledAttentionPlan *plan = new CtoxMpsTiledAttentionPlan();
        plan->tokens = tokens;
        plan->q_tile = q_tile;
        plan->k_tile = k_tile;
        plan->head_dim = head_dim;
        plan->heads_per_group = heads_per_group;
        plan->q_rows = q_tile * heads_per_group;
        plan->q_row_bytes = q_row_bytes;
        plan->k_row_bytes = k_row_bytes;
        plan->v_row_bytes = v_row_bytes;
        plan->score_row_bytes = score_row_bytes;
        plan->out_row_bytes = out_row_bytes;
        plan->qk = [[MPSMatrixMultiplication alloc]
            initWithDevice:device
             transposeLeft:NO
            transposeRight:NO
                resultRows:plan->q_rows
             resultColumns:k_tile
           interiorColumns:head_dim
                     alpha:(1.0 / sqrt((double)head_dim))
                      beta:0.0];
        plan->pv = [[MPSMatrixMultiplication alloc]
            initWithDevice:device
             transposeLeft:NO
            transposeRight:NO
                resultRows:plan->q_rows
             resultColumns:head_dim
           interiorColumns:k_tile
                     alpha:1.0
                      beta:0.0];
        return plan;
    }
}

extern "C" void ctox_mps_tiled_attention_plan_free(void *plan_ptr) {
    CtoxMpsTiledAttentionPlan *plan = static_cast<CtoxMpsTiledAttentionPlan *>(plan_ptr);
    delete plan;
}

extern "C" int ctox_mps_tiled_attention_encode_qk(
    void *plan_ptr,
    void *command_buffer_ptr,
    void *q_buffer_ptr,
    void *k_buffer_ptr,
    void *score_buffer_ptr,
    uint32_t q_block,
    uint32_t k_block
) {
    @autoreleasepool {
        CtoxMpsTiledAttentionPlan *plan = static_cast<CtoxMpsTiledAttentionPlan *>(plan_ptr);
        if (plan == nullptr) {
            return 0;
        }
        id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)command_buffer_ptr;
        id<MTLBuffer> qBuffer = (__bridge id<MTLBuffer>)q_buffer_ptr;
        id<MTLBuffer> kBuffer = (__bridge id<MTLBuffer>)k_buffer_ptr;
        id<MTLBuffer> scoreBuffer = (__bridge id<MTLBuffer>)score_buffer_ptr;

        MPSMatrixDescriptor *qDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:(plan->tokens * plan->heads_per_group)
                             columns:plan->head_dim
                            rowBytes:plan->q_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *kDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->head_dim
                             columns:plan->tokens
                            rowBytes:plan->k_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *scoreDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->q_rows
                             columns:plan->k_tile
                            rowBytes:plan->score_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrix *q = [[MPSMatrix alloc] initWithBuffer:qBuffer descriptor:qDesc];
        MPSMatrix *k = [[MPSMatrix alloc] initWithBuffer:kBuffer descriptor:kDesc];
        MPSMatrix *score = [[MPSMatrix alloc] initWithBuffer:scoreBuffer descriptor:scoreDesc];
        plan->qk.leftMatrixOrigin = MTLOriginMake(q_block * plan->q_rows, 0, 0);
        plan->qk.rightMatrixOrigin = MTLOriginMake(0, k_block * plan->k_tile, 0);
        plan->qk.resultMatrixOrigin = MTLOriginMake(0, 0, 0);
        [plan->qk encodeToCommandBuffer:commandBuffer leftMatrix:q rightMatrix:k resultMatrix:score];
        return 1;
    }
}

extern "C" int ctox_mps_tiled_attention_encode_pv(
    void *plan_ptr,
    void *command_buffer_ptr,
    void *prob_buffer_ptr,
    void *v_buffer_ptr,
    void *pv_buffer_ptr,
    uint32_t k_block
) {
    @autoreleasepool {
        CtoxMpsTiledAttentionPlan *plan = static_cast<CtoxMpsTiledAttentionPlan *>(plan_ptr);
        if (plan == nullptr) {
            return 0;
        }
        id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)command_buffer_ptr;
        id<MTLBuffer> probBuffer = (__bridge id<MTLBuffer>)prob_buffer_ptr;
        id<MTLBuffer> vBuffer = (__bridge id<MTLBuffer>)v_buffer_ptr;
        id<MTLBuffer> pvBuffer = (__bridge id<MTLBuffer>)pv_buffer_ptr;

        MPSMatrixDescriptor *probDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->q_rows
                             columns:plan->k_tile
                            rowBytes:plan->score_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *vDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->head_dim
                            rowBytes:plan->v_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *pvDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->q_rows
                             columns:plan->head_dim
                            rowBytes:plan->out_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrix *prob = [[MPSMatrix alloc] initWithBuffer:probBuffer descriptor:probDesc];
        MPSMatrix *v = [[MPSMatrix alloc] initWithBuffer:vBuffer descriptor:vDesc];
        MPSMatrix *pv = [[MPSMatrix alloc] initWithBuffer:pvBuffer descriptor:pvDesc];
        plan->pv.leftMatrixOrigin = MTLOriginMake(0, 0, 0);
        plan->pv.rightMatrixOrigin = MTLOriginMake(k_block * plan->k_tile, 0, 0);
        plan->pv.resultMatrixOrigin = MTLOriginMake(0, 0, 0);
        [plan->pv encodeToCommandBuffer:commandBuffer leftMatrix:prob rightMatrix:v resultMatrix:pv];
        return 1;
    }
}

extern "C" int ctox_mps_delta_project_encode(
    void *plan_ptr,
    void *command_buffer_ptr,
    void *x_buffer_ptr,
    void *weight_buffer_ptr,
    void *out_buffer_ptr
) {
    @autoreleasepool {
        CtoxMpsDeltaProjectPlan *plan = static_cast<CtoxMpsDeltaProjectPlan *>(plan_ptr);
        if (plan == nullptr) {
            return 0;
        }
        id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)command_buffer_ptr;
        id<MTLBuffer> xBuffer = (__bridge id<MTLBuffer>)x_buffer_ptr;
        id<MTLBuffer> weightBuffer = (__bridge id<MTLBuffer>)weight_buffer_ptr;
        id<MTLBuffer> outBuffer = (__bridge id<MTLBuffer>)out_buffer_ptr;

        MPSMatrixDescriptor *xDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->hidden
                            rowBytes:plan->x_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *wDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->hidden
                             columns:plan->qkvz_rows
                            rowBytes:plan->weight_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *outDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->qkvz_rows
                            rowBytes:plan->out_row_bytes
                            dataType:MPSDataTypeFloat32];
        MPSMatrix *x = [[MPSMatrix alloc] initWithBuffer:xBuffer descriptor:xDesc];
        MPSMatrix *w = [[MPSMatrix alloc] initWithBuffer:weightBuffer descriptor:wDesc];
        MPSMatrix *out = [[MPSMatrix alloc] initWithBuffer:outBuffer descriptor:outDesc];
        [plan->project encodeToCommandBuffer:commandBuffer leftMatrix:x rightMatrix:w resultMatrix:out];
        return 1;
    }
}

extern "C" void *ctox_mps_ffn_plan_new(
    void *device_ptr,
    uint32_t tokens,
    uint32_t hidden,
    uint32_t intermediate,
    uint32_t x_row_bytes,
    uint32_t gate_up_weight_row_bytes,
    uint32_t gate_up_row_bytes,
    uint32_t act_row_bytes,
    uint32_t down_weight_row_bytes,
    uint32_t out_row_bytes
) {
    @autoreleasepool {
        id<MTLDevice> device = (__bridge id<MTLDevice>)device_ptr;
        if (!MPSSupportsMTLDevice(device)) {
            return nullptr;
        }
        CtoxMpsFfnPlan *plan = new CtoxMpsFfnPlan();
        plan->tokens = tokens;
        plan->hidden = hidden;
        plan->intermediate = intermediate;
        plan->x_row_bytes = x_row_bytes;
        plan->gate_up_weight_row_bytes = gate_up_weight_row_bytes;
        plan->gate_up_row_bytes = gate_up_row_bytes;
        plan->act_row_bytes = act_row_bytes;
        plan->down_weight_row_bytes = down_weight_row_bytes;
        plan->out_row_bytes = out_row_bytes;
        plan->gate_up = [[MPSMatrixMultiplication alloc]
            initWithDevice:device
             transposeLeft:NO
            transposeRight:NO
                resultRows:tokens
             resultColumns:(intermediate * 2)
           interiorColumns:hidden
                     alpha:1.0
                      beta:0.0];
        plan->down = [[MPSMatrixMultiplication alloc]
            initWithDevice:device
             transposeLeft:NO
            transposeRight:NO
                resultRows:tokens
             resultColumns:hidden
           interiorColumns:intermediate
                     alpha:1.0
                      beta:0.0];
        return plan;
    }
}

extern "C" void ctox_mps_ffn_plan_free(void *plan_ptr) {
    CtoxMpsFfnPlan *plan = static_cast<CtoxMpsFfnPlan *>(plan_ptr);
    delete plan;
}

extern "C" int ctox_mps_ffn_encode_gate_up(
    void *plan_ptr,
    void *command_buffer_ptr,
    void *x_buffer_ptr,
    void *gate_up_weight_buffer_ptr,
    void *gate_up_out_buffer_ptr
) {
    @autoreleasepool {
        CtoxMpsFfnPlan *plan = static_cast<CtoxMpsFfnPlan *>(plan_ptr);
        if (plan == nullptr) {
            return 0;
        }
        id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)command_buffer_ptr;
        id<MTLBuffer> xBuffer = (__bridge id<MTLBuffer>)x_buffer_ptr;
        id<MTLBuffer> gateUpWeightBuffer = (__bridge id<MTLBuffer>)gate_up_weight_buffer_ptr;
        id<MTLBuffer> gateUpOutBuffer = (__bridge id<MTLBuffer>)gate_up_out_buffer_ptr;

        MPSMatrixDescriptor *xDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->hidden
                            rowBytes:plan->x_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *wDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->hidden
                             columns:(plan->intermediate * 2)
                            rowBytes:plan->gate_up_weight_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *yDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:(plan->intermediate * 2)
                            rowBytes:plan->gate_up_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrix *x = [[MPSMatrix alloc] initWithBuffer:xBuffer descriptor:xDesc];
        MPSMatrix *w = [[MPSMatrix alloc] initWithBuffer:gateUpWeightBuffer descriptor:wDesc];
        MPSMatrix *y = [[MPSMatrix alloc] initWithBuffer:gateUpOutBuffer descriptor:yDesc];
        [plan->gate_up encodeToCommandBuffer:commandBuffer leftMatrix:x rightMatrix:w resultMatrix:y];
        return 1;
    }
}

extern "C" int ctox_mps_ffn_encode_down(
    void *plan_ptr,
    void *command_buffer_ptr,
    void *act_buffer_ptr,
    void *down_weight_buffer_ptr,
    void *out_buffer_ptr
) {
    @autoreleasepool {
        CtoxMpsFfnPlan *plan = static_cast<CtoxMpsFfnPlan *>(plan_ptr);
        if (plan == nullptr) {
            return 0;
        }
        id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)command_buffer_ptr;
        id<MTLBuffer> actBuffer = (__bridge id<MTLBuffer>)act_buffer_ptr;
        id<MTLBuffer> downWeightBuffer = (__bridge id<MTLBuffer>)down_weight_buffer_ptr;
        id<MTLBuffer> outBuffer = (__bridge id<MTLBuffer>)out_buffer_ptr;

        MPSMatrixDescriptor *actDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->intermediate
                            rowBytes:plan->act_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *wDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->intermediate
                             columns:plan->hidden
                            rowBytes:plan->down_weight_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrixDescriptor *outDesc = [MPSMatrixDescriptor
            matrixDescriptorWithRows:plan->tokens
                             columns:plan->hidden
                            rowBytes:plan->out_row_bytes
                            dataType:MPSDataTypeFloat16];
        MPSMatrix *act = [[MPSMatrix alloc] initWithBuffer:actBuffer descriptor:actDesc];
        MPSMatrix *w = [[MPSMatrix alloc] initWithBuffer:downWeightBuffer descriptor:wDesc];
        MPSMatrix *out = [[MPSMatrix alloc] initWithBuffer:outBuffer descriptor:outDesc];
        [plan->down encodeToCommandBuffer:commandBuffer leftMatrix:act rightMatrix:w resultMatrix:out];
        return 1;
    }
}
