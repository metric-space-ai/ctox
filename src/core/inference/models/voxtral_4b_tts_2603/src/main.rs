use ctox_voxtral_4b_tts_2603::model::names;
use ctox_voxtral_4b_tts_2603::safetensors::SafeTensors;
use ctox_voxtral_4b_tts_2603::tensor::DType;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: voxtral-rs-inspect <model-dir|consolidated.safetensors>");
            std::process::exit(2);
        }
    };
    let weights_path = if path.ends_with(".safetensors") {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(path).join("consolidated.safetensors")
    };
    let st = match SafeTensors::open(&weights_path) {
        Ok(st) => st,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    println!("file: {}", weights_path.display());
    println!("tensors: {}", st.tensors().len());
    let bf16 = st
        .tensors()
        .iter()
        .filter(|t| t.dtype == DType::BF16)
        .count();
    let f32n = st
        .tensors()
        .iter()
        .filter(|t| t.dtype == DType::F32)
        .count();
    println!("dtype counts: BF16={bf16}, F32={f32n}");
    for key in [names::TOK_EMBEDDINGS, names::ADAPTER_L0, names::ADAPTER_L1] {
        match st.find(key) {
            Some(t) => println!("{key}: {:?} {:?}", t.dtype, t.shape),
            None => println!("{key}: missing"),
        }
    }
}
