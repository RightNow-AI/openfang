use leptos::prelude::*;

use crate::models::RegistryStats;

/// Server function — fetches registry stats from the database.
/// Leptos compiles this to an Axum route on the server and a fetch() call on the client.
#[server(GetRegistryStats, "/api/leptos")]
pub async fn get_registry_stats() -> Result<RegistryStats, ServerFnError> {
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    store
        .get_stats()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

/// Stats bar component — shows total packages, versions, installs, and publishers.
#[component]
pub fn StatsBar() -> impl IntoView {
    let stats = Resource::new(|| (), |_| get_registry_stats());

    view! {
        <Suspense fallback=|| view! { <div class="h-12"/> }>
            {move || stats.get().map(|result| {
                match result {
                    Ok(s) => view! {
                        <div class="flex items-center justify-center gap-8 flex-wrap">
                            <StatItem label="Packages" value=s.total_packages/>
                            <StatItem label="Versions" value=s.total_versions/>
                            <StatItem label="Installs" value=s.total_installs/>
                            <StatItem label="Publishers" value=s.total_publishers/>
                        </div>
                    }.into_any(),
                    Err(_) => view! { <div/> }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn StatItem(label: &'static str, value: u64) -> impl IntoView {
    view! {
        <div class="text-center">
            <div class="text-2xl font-bold text-sky-400">
                {value.to_string()}
            </div>
            <div class="text-xs text-slate-500 mt-0.5">
                {label}
            </div>
        </div>
    }
}
