// mod_e.rs — cross-file caller of old_name.
pub fn caller_e(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 5
}

pub fn helper_e(x: i32) -> i32 {
    caller_e(x) * 5
}
