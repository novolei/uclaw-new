// mod_g.rs — cross-file caller of old_name.
pub fn caller_g(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 7
}

pub fn helper_g(x: i32) -> i32 {
    caller_g(x) * 7
}
