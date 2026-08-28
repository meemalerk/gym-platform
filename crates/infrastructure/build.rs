fn main() {
    // `sqlx::migrate!` embeds ../../migrations at compile time, but the proc
    // macro only tracks the files that existed when it last expanded — it cannot
    // register a watch on the directory, so ADDING a migration does not rebuild
    // this crate. The stale binary then refuses to start with:
    //
    //   migration <N> was previously applied but is missing in the resolved migrations
    //
    // (Observed live: verify-execution.sh went 25-green to 23-red in one run
    // because the freshly-applied migration was absent from the embedded set.)
    // This line restores the directory watch.
    println!("cargo:rerun-if-changed=../../migrations");
}
