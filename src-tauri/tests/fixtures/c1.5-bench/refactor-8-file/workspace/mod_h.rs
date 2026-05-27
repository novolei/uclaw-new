// mod_h.rs — cross-file caller of old_name.
pub fn caller_h(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 8
}

pub fn helper_h(x: i32) -> i32 {
    caller_h(x) * 8
}
