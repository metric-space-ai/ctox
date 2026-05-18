#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("list_metal_counters is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{
        MTLCounter, MTLCounterSamplingPoint, MTLCounterSet, MTLCreateSystemDefaultDevice, MTLDevice,
    };

    let raw = unsafe { MTLCreateSystemDefaultDevice() };
    if raw.is_null() {
        return Err("MTLCreateSystemDefaultDevice returned null".to_string());
    }
    let device: Retained<ProtocolObject<dyn MTLDevice>> =
        unsafe { Retained::from_raw(raw).ok_or_else(|| "failed to retain MTLDevice".to_string())? };

    println!("metal counter sampling support:");
    for (name, point) in [
        ("stage", MTLCounterSamplingPoint::AtStageBoundary),
        ("draw", MTLCounterSamplingPoint::AtDrawBoundary),
        ("dispatch", MTLCounterSamplingPoint::AtDispatchBoundary),
        (
            "tile_dispatch",
            MTLCounterSamplingPoint::AtTileDispatchBoundary,
        ),
        ("blit", MTLCounterSamplingPoint::AtBlitBoundary),
    ] {
        println!("  {name}: {}", device.supportsCounterSampling(point));
    }

    let Some(counter_sets) = (unsafe { device.counterSets() }) else {
        println!("counter_sets: none");
        return Ok(());
    };

    println!("counter_sets: {}", counter_sets.len());
    for set_idx in 0..counter_sets.len() {
        let set = unsafe { counter_sets.objectAtIndex(set_idx) };
        let set_name = unsafe { set.name() };
        let counters = unsafe { set.counters() };
        println!("set[{set_idx}]: {} counters={}", set_name, counters.len());
        for counter_idx in 0..counters.len() {
            let counter = unsafe { counters.objectAtIndex(counter_idx) };
            let counter_name = unsafe { counter.name() };
            println!("  counter[{counter_idx}]: {counter_name}");
        }
    }

    Ok(())
}
