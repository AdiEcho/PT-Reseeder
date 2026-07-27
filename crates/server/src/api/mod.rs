// Hand-written REST endpoints.
//
// This layer is an internal implementation detail, not a public API: the browser
// talks to the app through Leptos server functions. Only two modules remain —
// `health` for liveness checks, and `repost` for the three endpoints the hydrated
// page calls directly (review / submit / autofill), which have no server-fn
// equivalent that also writes `adapted_info_json`.
pub mod health;
pub mod repost;
