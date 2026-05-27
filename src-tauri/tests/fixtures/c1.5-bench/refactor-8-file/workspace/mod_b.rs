// mod_b.rs — cross-file caller of old_name.
pub fn caller_b(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 2
}

pub fn helper_b(x: i32) -> i32 {
    caller_b(x) * 2
}
