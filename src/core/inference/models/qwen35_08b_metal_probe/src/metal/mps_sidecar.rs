//! Minimal MPS bridge for matrix-backend sidecar experiments.

use std::{ffi::c_void, ptr::NonNull};

use crate::metal::ffi::{Buffer, CommandBuffer, Device};

extern "C" {
    fn ctox_mps_device_supports(device: *mut c_void) -> i32;
    fn ctox_mps_ffn_plan_new(
        device: *mut c_void,
        tokens: u32,
        hidden: u32,
        intermediate: u32,
        x_row_bytes: u32,
        gate_up_weight_row_bytes: u32,
        gate_up_row_bytes: u32,
        act_row_bytes: u32,
        down_weight_row_bytes: u32,
        out_row_bytes: u32,
    ) -> *mut c_void;
    fn ctox_mps_ffn_plan_free(plan: *mut c_void);
    fn ctox_mps_delta_project_plan_new(
        device: *mut c_void,
        tokens: u32,
        hidden: u32,
        qkvz_rows: u32,
        x_row_bytes: u32,
        weight_row_bytes: u32,
        out_row_bytes: u32,
    ) -> *mut c_void;
    fn ctox_mps_delta_project_plan_free(plan: *mut c_void);
    fn ctox_mps_tiled_attention_plan_new(
        device: *mut c_void,
        tokens: u32,
        q_tile: u32,
        k_tile: u32,
        head_dim: u32,
        heads_per_group: u32,
        q_row_bytes: u32,
        k_row_bytes: u32,
        v_row_bytes: u32,
        score_row_bytes: u32,
        out_row_bytes: u32,
    ) -> *mut c_void;
    fn ctox_mps_tiled_attention_plan_free(plan: *mut c_void);
    fn ctox_mps_tiled_attention_encode_qk(
        plan: *mut c_void,
        command_buffer: *mut c_void,
        q_buffer: *mut c_void,
        k_buffer: *mut c_void,
        score_buffer: *mut c_void,
        q_block: u32,
        k_block: u32,
    ) -> i32;
    fn ctox_mps_tiled_attention_encode_pv(
        plan: *mut c_void,
        command_buffer: *mut c_void,
        prob_buffer: *mut c_void,
        v_buffer: *mut c_void,
        pv_buffer: *mut c_void,
        k_block: u32,
    ) -> i32;
    fn ctox_mps_delta_project_encode(
        plan: *mut c_void,
        command_buffer: *mut c_void,
        x_buffer: *mut c_void,
        weight_buffer: *mut c_void,
        out_buffer: *mut c_void,
    ) -> i32;
    fn ctox_mps_ffn_encode_gate_up(
        plan: *mut c_void,
        command_buffer: *mut c_void,
        x_buffer: *mut c_void,
        gate_up_weight_buffer: *mut c_void,
        gate_up_out_buffer: *mut c_void,
    ) -> i32;
    fn ctox_mps_ffn_encode_down(
        plan: *mut c_void,
        command_buffer: *mut c_void,
        act_buffer: *mut c_void,
        down_weight_buffer: *mut c_void,
        out_buffer: *mut c_void,
    ) -> i32;
}

pub fn device_supports_mps(device: &Device) -> bool {
    unsafe { ctox_mps_device_supports(device.raw_device_ptr()) != 0 }
}

pub struct MpsFfnPlan {
    raw: NonNull<c_void>,
}

unsafe impl Send for MpsFfnPlan {}
unsafe impl Sync for MpsFfnPlan {}

impl MpsFfnPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &Device,
        tokens: usize,
        hidden: usize,
        intermediate: usize,
        x_row_bytes: usize,
        gate_up_weight_row_bytes: usize,
        gate_up_row_bytes: usize,
        act_row_bytes: usize,
        down_weight_row_bytes: usize,
        out_row_bytes: usize,
    ) -> Result<Self, String> {
        let raw = unsafe {
            ctox_mps_ffn_plan_new(
                device.raw_device_ptr(),
                as_u32(tokens, "tokens")?,
                as_u32(hidden, "hidden")?,
                as_u32(intermediate, "intermediate")?,
                as_u32(x_row_bytes, "x_row_bytes")?,
                as_u32(gate_up_weight_row_bytes, "gate_up_weight_row_bytes")?,
                as_u32(gate_up_row_bytes, "gate_up_row_bytes")?,
                as_u32(act_row_bytes, "act_row_bytes")?,
                as_u32(down_weight_row_bytes, "down_weight_row_bytes")?,
                as_u32(out_row_bytes, "out_row_bytes")?,
            )
        };
        let raw = NonNull::new(raw).ok_or_else(|| "failed to create MPS FFN plan".to_owned())?;
        Ok(Self { raw })
    }

    pub fn encode_gate_up(
        &self,
        command_buffer: &CommandBuffer,
        x: &Buffer,
        gate_up_weight: &Buffer,
        gate_up_out: &Buffer,
    ) -> Result<(), String> {
        let ok = unsafe {
            ctox_mps_ffn_encode_gate_up(
                self.raw.as_ptr(),
                command_buffer.raw_command_buffer_ptr(),
                x.raw_buffer_ptr(),
                gate_up_weight.raw_buffer_ptr(),
                gate_up_out.raw_buffer_ptr(),
            )
        };
        if ok == 0 {
            Err("MPS FFN gate_up encode failed".to_owned())
        } else {
            Ok(())
        }
    }

    pub fn encode_down(
        &self,
        command_buffer: &CommandBuffer,
        act: &Buffer,
        down_weight: &Buffer,
        out: &Buffer,
    ) -> Result<(), String> {
        let ok = unsafe {
            ctox_mps_ffn_encode_down(
                self.raw.as_ptr(),
                command_buffer.raw_command_buffer_ptr(),
                act.raw_buffer_ptr(),
                down_weight.raw_buffer_ptr(),
                out.raw_buffer_ptr(),
            )
        };
        if ok == 0 {
            Err("MPS FFN down encode failed".to_owned())
        } else {
            Ok(())
        }
    }
}

pub struct MpsDeltaProjectPlan {
    raw: NonNull<c_void>,
}

unsafe impl Send for MpsDeltaProjectPlan {}
unsafe impl Sync for MpsDeltaProjectPlan {}

pub struct MpsTiledAttentionPlan {
    raw: NonNull<c_void>,
}

unsafe impl Send for MpsTiledAttentionPlan {}
unsafe impl Sync for MpsTiledAttentionPlan {}

impl MpsDeltaProjectPlan {
    pub fn new(
        device: &Device,
        tokens: usize,
        hidden: usize,
        qkvz_rows: usize,
        x_row_bytes: usize,
        weight_row_bytes: usize,
        out_row_bytes: usize,
    ) -> Result<Self, String> {
        let raw = unsafe {
            ctox_mps_delta_project_plan_new(
                device.raw_device_ptr(),
                as_u32(tokens, "tokens")?,
                as_u32(hidden, "hidden")?,
                as_u32(qkvz_rows, "qkvz_rows")?,
                as_u32(x_row_bytes, "x_row_bytes")?,
                as_u32(weight_row_bytes, "weight_row_bytes")?,
                as_u32(out_row_bytes, "out_row_bytes")?,
            )
        };
        let raw = NonNull::new(raw)
            .ok_or_else(|| "failed to create MPS DeltaNet project plan".to_owned())?;
        Ok(Self { raw })
    }

    pub fn encode(
        &self,
        command_buffer: &CommandBuffer,
        x: &Buffer,
        weight: &Buffer,
        out: &Buffer,
    ) -> Result<(), String> {
        let ok = unsafe {
            ctox_mps_delta_project_encode(
                self.raw.as_ptr(),
                command_buffer.raw_command_buffer_ptr(),
                x.raw_buffer_ptr(),
                weight.raw_buffer_ptr(),
                out.raw_buffer_ptr(),
            )
        };
        if ok == 0 {
            Err("MPS DeltaNet project encode failed".to_owned())
        } else {
            Ok(())
        }
    }
}

impl MpsTiledAttentionPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &Device,
        tokens: usize,
        q_tile: usize,
        k_tile: usize,
        head_dim: usize,
        heads_per_group: usize,
        q_row_bytes: usize,
        k_row_bytes: usize,
        v_row_bytes: usize,
        score_row_bytes: usize,
        out_row_bytes: usize,
    ) -> Result<Self, String> {
        let raw = unsafe {
            ctox_mps_tiled_attention_plan_new(
                device.raw_device_ptr(),
                as_u32(tokens, "tokens")?,
                as_u32(q_tile, "q_tile")?,
                as_u32(k_tile, "k_tile")?,
                as_u32(head_dim, "head_dim")?,
                as_u32(heads_per_group, "heads_per_group")?,
                as_u32(q_row_bytes, "q_row_bytes")?,
                as_u32(k_row_bytes, "k_row_bytes")?,
                as_u32(v_row_bytes, "v_row_bytes")?,
                as_u32(score_row_bytes, "score_row_bytes")?,
                as_u32(out_row_bytes, "out_row_bytes")?,
            )
        };
        let raw = NonNull::new(raw)
            .ok_or_else(|| "failed to create MPS tiled attention plan".to_owned())?;
        Ok(Self { raw })
    }

    pub fn encode_qk(
        &self,
        command_buffer: &CommandBuffer,
        q: &Buffer,
        k: &Buffer,
        score: &Buffer,
        q_block: usize,
        k_block: usize,
    ) -> Result<(), String> {
        let ok = unsafe {
            ctox_mps_tiled_attention_encode_qk(
                self.raw.as_ptr(),
                command_buffer.raw_command_buffer_ptr(),
                q.raw_buffer_ptr(),
                k.raw_buffer_ptr(),
                score.raw_buffer_ptr(),
                as_u32(q_block, "q_block")?,
                as_u32(k_block, "k_block")?,
            )
        };
        if ok == 0 {
            Err("MPS tiled attention QK encode failed".to_owned())
        } else {
            Ok(())
        }
    }

    pub fn encode_pv(
        &self,
        command_buffer: &CommandBuffer,
        prob: &Buffer,
        v: &Buffer,
        pv: &Buffer,
        k_block: usize,
    ) -> Result<(), String> {
        let ok = unsafe {
            ctox_mps_tiled_attention_encode_pv(
                self.raw.as_ptr(),
                command_buffer.raw_command_buffer_ptr(),
                prob.raw_buffer_ptr(),
                v.raw_buffer_ptr(),
                pv.raw_buffer_ptr(),
                as_u32(k_block, "k_block")?,
            )
        };
        if ok == 0 {
            Err("MPS tiled attention PV encode failed".to_owned())
        } else {
            Ok(())
        }
    }
}

impl Drop for MpsDeltaProjectPlan {
    fn drop(&mut self) {
        unsafe {
            ctox_mps_delta_project_plan_free(self.raw.as_ptr());
        }
    }
}

impl Drop for MpsFfnPlan {
    fn drop(&mut self) {
        unsafe {
            ctox_mps_ffn_plan_free(self.raw.as_ptr());
        }
    }
}

impl Drop for MpsTiledAttentionPlan {
    fn drop(&mut self) {
        unsafe {
            ctox_mps_tiled_attention_plan_free(self.raw.as_ptr());
        }
    }
}

fn as_u32(value: usize, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{label} exceeds u32"))
}
