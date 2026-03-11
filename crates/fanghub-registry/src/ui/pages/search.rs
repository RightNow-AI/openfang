use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::models::{SearchQuery, SearchResult, SortOrder};
use crate::ui::components::PackageCard;

/// Server function — executes a search query against the registry.
#[server(SearchPackages, "/api/leptos")]
pub async fn search_packages(query: SearchQuery) -> Result<Vec<SearchResult>, ServerFnError> {
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    let response = store
        .search_packages(&query)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(response.0)
}

#[component]
pub fn SearchPage() -> impl IntoView {
    let query_map = use_query_map();

    // Derive search parameters from URL query string (reactive)
    let search_query = move || {
        let map = query_map.get();
        SearchQuery {
            q: map.get("q").map(|s| s.to_string()),
            category: map.get("category").map(|s| s.to_string()),
            tag: map.get("tag").map(|s| s.to_string()),
            sort: map.get("sort").map(|s| match s.as_str() {
                "name" => SortOrder::Name,
                "updated" => SortOrder::Updated,
                _ => SortOrder::Installs,
            }),
            page: map.get("page").and_then(|s| s.parse().ok()),
            per_page: Some(24),
        }
    };

    let results = Resource::new(search_query, search_packages);

    let q_display = move || {
        query_map
            .get()
            .get("q")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    view! {
        <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <div class="mb-8">
                <h1 class="text-2xl font-bold text-slate-100 mb-1">
                    {move || {
                        let q = q_display();
                        if q.is_empty() {
                            "All Packages".to_string()
                        } else {
                            format!("Search: {q}")
                        }
                    }}
                </h1>
                <p class="text-slate-500 text-sm">"Browse and discover Hand packages for your OpenFang agent"</p>
            </div>

            // Sort controls
            <div class="flex items-center gap-3 mb-6">
                <span class="text-sm text-slate-500">"Sort by:"</span>
                <SortLink label="Most Installed" value="installs"/>
                <SortLink label="Recently Updated" value="updated"/>
                <SortLink label="Name" value="name"/>
            </div>

            // Results grid
            <Suspense fallback=|| view! {
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                    {(0..12).map(|_| view! {
                        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 h-40 animate-pulse"/>
                    }).collect_view()}
                </div>
            }>
                {move || results.get().map(|result| match result {
                    Ok(pkgs) if pkgs.is_empty() => view! {
                        <div class="text-center py-20 text-slate-500">
                            <svg class="w-12 h-12 mx-auto mb-4 text-slate-700" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                                    d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
                            </svg>
                            <p class="text-lg font-medium mb-2">"No packages found"</p>
                            <p class="text-sm">"Try a different search term or browse all packages"</p>
                        </div>
                    }.into_any(),
                    Ok(pkgs) => view! {
                        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                            {pkgs.into_iter().map(|pkg| view! {
                                <PackageCard pkg=pkg/>
                            }).collect_view()}
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="text-center py-20 text-red-400">
                            <p>"Error loading packages: " {e.to_string()}</p>
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn SortLink(label: &'static str, value: &'static str) -> impl IntoView {
    let query_map = use_query_map();
    let is_active = move || {
        let map = query_map.get();
        let current = map.get("sort").map(|s| s.to_string()).unwrap_or_else(|| "installs".to_string());
        current.as_str() == value
    };

    // Build href with updated sort param — use into_iter() which is supported on ParamsMap
    let href = move || {
        let map = query_map.get();
        let mut parts: Vec<String> = map
            .into_iter()
            .filter(|(k, _)| k.as_ref() != "sort")
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(&v)))
            .collect();
        parts.push(format!("sort={value}"));
        format!("/search?{}", parts.join("&"))
    };

    view! {
        <a
            href=href
            class=move || if is_active() {
                "px-3 py-1.5 rounded-lg bg-sky-900 text-sky-300 text-xs font-medium border border-sky-700 no-underline"
            } else {
                "px-3 py-1.5 rounded-lg bg-slate-900 text-slate-400 text-xs font-medium border border-slate-700 hover:border-slate-500 transition-colors no-underline"
            }
        >
            {label}
        </a>
    }
}
