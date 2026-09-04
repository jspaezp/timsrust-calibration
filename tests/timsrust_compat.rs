use timsrust::core::{Converter, Mz, TofIndex};
use timsrust_calibration::CalibratedTof2MzConverter;

// Compile-time proof that the converter implements the trait and coordinate
// types re-exported by timsrust 0.6.5, rather than an incompatible core copy.
fn accepts_timsrust_converter<C: Converter<TofIndex, Mz>>() {}

#[test]
fn converter_uses_timsrust_0_6_5_core_types() {
    accepts_timsrust_converter::<CalibratedTof2MzConverter>();
}
