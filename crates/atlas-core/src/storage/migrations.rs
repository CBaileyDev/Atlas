//! Embedded refinery migrations. Files live under `migrations/` next to
//! this Cargo manifest. The `embed_migrations!` macro picks them up at
//! compile time and walks them in lexicographic order.

// The macro injects a nested module containing a `runner()` function.
// We re-export the runner at this module's root so callers can write
// `crate::storage::migrations::runner()` rather than the doubly-nested path.
mod _embedded {
    refinery::embed_migrations!("migrations");
    pub use migrations::runner;
}

pub use _embedded::runner;
