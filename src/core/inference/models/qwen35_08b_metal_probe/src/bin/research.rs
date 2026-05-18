use ctox_qwen35_08b_metal_probe::{LayerKind, ResearchPlan, QWEN35_08B};

fn main() {
    let shape = QWEN35_08B;
    let plan = ResearchPlan::default();

    println!("qwen35-08b-metal-research");
    println!("model: {}", shape.model);
    println!("hidden: {}", shape.hidden_size);
    println!("vocab: {}", shape.vocab_size);
    println!("layers: {}", shape.n_layers);
    println!(
        "layer mix: {} DeltaNet, {} full attention",
        shape.n_deltanet_layers(),
        shape.n_full_attention_layers()
    );
    println!(
        "lm_head_fp16_bytes: {} ({:.2} MiB)",
        shape.lm_head_fp16_bytes(),
        shape.lm_head_fp16_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "approx_fp16_weight_bytes: {} ({:.2} GiB)",
        shape.approximate_fp16_weight_bytes(),
        shape.approximate_fp16_weight_bytes() as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("layout:");
    for layer in 0..shape.n_layers {
        let tag = match shape.layer_kind(layer) {
            LayerKind::GatedDeltaNet => "D",
            LayerKind::FullAttention => "A",
        };
        print!("{tag}");
        if layer + 1 != shape.n_layers {
            print!(" ");
        }
    }
    println!();
    println!(
        "research gates: {} passed, {} pending",
        plan.passed_count(),
        plan.pending_count()
    );
}
