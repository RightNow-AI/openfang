use leptos::prelude::*;
use urlencoding;

use crate::models::{SearchQuery, SearchResult, SortOrder};
use crate::ui::components::{PackageCard, StatsBar};

const CATEGORIES: &[&str] = &[
    "Productivity",
    "Communication",
    "Information",
    "Automation",
    "Finance",
    "Development",
    "Media",
    "Analytics",
];

/// Server function — fetches the top 6 most-installed packages for the home page.
#[server(GetFeaturedPackages, "/api/leptos")]
pub async fn get_featured_packages() -> Result<Vec<SearchResult>, ServerFnError> {
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    let q = SearchQuery {
        q: None,
        category: None,
        tag: None,
        sort: Some(SortOrder::Installs),
        page: Some(1),
        per_page: Some(6),
    };
    let response = store
        .search_packages(&q)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(response.0)
}

#[component]
pub fn HomePage() -> impl IntoView {
    let featured = Resource::new(|| (), |_| get_featured_packages());

    view! {
        <div>
            // Hero section
            <section class="relative overflow-hidden bg-gradient-to-b from-slate-900 to-slate-950 border-b border-slate-800">
                <div class="absolute inset-0" style="background: radial-gradient(ellipse at top, rgba(14,165,233,0.08) 0%, transparent 60%)"/>
                <div class="relative max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-20 text-center">
                    <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-sky-900/50 border border-sky-800 text-sky-300 text-xs font-medium mb-6">
                        <span class="w-1.5 h-1.5 rounded-full bg-sky-400"/>
                        "OpenFang v0.3.33 — Phase 11"
                    </div>

                    <h1 class="text-4xl sm:text-5xl font-bold text-slate-100 mb-4">
                        "The Hand Package Marketplace"
                    </h1>
                    <p class="text-lg text-slate-400 mb-8 max-w-2xl mx-auto">
                        "Discover, install, and publish "
                        <strong class="text-slate-300">"Hand packages"</strong>
                        " for your OpenFang agent. Extend your AI with community-built capabilities."
                    </p>

                    <form method="GET" action="/search" class="flex gap-3 max-w-xl mx-auto mb-10">
                        <input
                            type="search"
                            name="q"
                            placeholder="Search packages... (e.g. weather, email, calendar)"
                            class="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500 focus:border-transparent text-sm"
                            autofocus
                        />
                        <button
                            type="submit"
                            class="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white font-medium text-sm transition-colors shrink-0"
                        >
                            "Search"
                        </button>
                    </form>

                    <div class="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-slate-900 border border-slate-700 text-sm font-mono text-slate-400">
                        <span class="text-slate-600">"$"</span>
                        <span>"fang install weather-hand"</span>
                    </div>
                </div>
            </section>

            // Stats bar
            <section class="border-b border-slate-800 py-8">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <StatsBar/>
                </div>
            </section>

            // Categories
            <section class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
                <h2 class="text-xl font-semibold text-slate-100 mb-6">"Browse by Category"</h2>
                <div class="flex flex-wrap gap-2">
                    {CATEGORIES.iter().map(|cat| {
                        let href = format!("/search?category={}", urlencoding::encode(cat));
                        view! {
                            <a
                                href=href
                                class="px-4 py-2 rounded-lg bg-slate-900 border border-slate-800 hover:border-sky-700 hover:text-sky-300 text-slate-300 text-sm transition-colors no-underline"
                            >
                                {*cat}
                            </a>
                        }
                    }).collect_view()}
                </div>
            </section>

            // Featured packages
            <section class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 pb-16">
                <div class="flex items-center justify-between mb-6">
                    <h2 class="text-xl font-semibold text-slate-100">"Most Installed"</h2>
                    <a href="/search" class="text-sm text-sky-400 hover:text-sky-300 transition-colors no-underline">
                        "View all →"
                    </a>
                </div>

                <Suspense fallback=|| view! {
                    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                        {(0..6).map(|_| view! {
                            <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 h-40 animate-pulse"/>
                        }).collect_view()}
                    </div>
                }>
                    {move || featured.get().map(|result| match result {
                        Ok(pkgs) if pkgs.is_empty() => view! {
                            <div class="text-center py-16 text-slate-500">
                                <p class="text-lg mb-2">"No packages published yet"</p>
                                <p class="text-sm">
                                    "Be the first! "
                                    <a
                                        href="https://github.com/ParadiseAI/maestro-legacy/blob/main/docs/fanghub-publishing-guide.md"
                                        class="text-sky-400 hover:underline"
                                        target="_blank"
                                        rel="noopener noreferrer"
                                    >
                                        "Read the publishing guide"
                                    </a>
                                </p>
                            </div>
                        }.into_any(),
                        Ok(pkgs) => view! {
                            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                                {pkgs.into_iter().map(|pkg| view! {
                                    <PackageCard pkg=pkg/>
                                }).collect_view()}
                            </div>
                        }.into_any(),
                        Err(_) => view! { <div/> }.into_any(),
                    })}
                </Suspense>
            </section>
        </div>
    }
}
