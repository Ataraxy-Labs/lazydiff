fn main() {
    println!("cargo:rerun-if-env-changed=LAZYDIFF_CONVEX_URL");
    println!("cargo:rerun-if-env-changed=LAZYDIFF_CONVEX_HTTP_URL");
}
