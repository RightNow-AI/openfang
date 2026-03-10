use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::models::{HandPackage, PackageVersion};

/// Server function — fetches full package details.
#[server(GetPackageDetail, "/api/leptos")]
pub async fn get_package_detail(package_id: String) -> Result<HandPackage, ServerFnError> {
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    store
        .get_package(&package_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new(format!("Package '{package_id}' not found")))
}

/// Server function — fetches all versions of a package.
#[server(GetPackageVersions, "/api/leptos")]
pub async fn get_package_versions(package_id: String) -> Result<Vec<PackageVersion>, ServerFnError> {
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    store
        .get_versions(&package_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[component]
pub fn PackagePage() -> impl IntoView {
    let params = use_params_map();
    let package_id = move || {
        params
            .get()
            .get("package_id")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    let package = Resource::new(package_id, get_package_detail);
    let versions = Resource::new(package_id, get_package_versions);

    view! {
        <div class="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <Suspense fallback=|| view! {
                <div class="animate-pulse space-y-4">
                    <div class="h-8 bg-slate-800 rounded w-64"/>
                    <div class="h-4 bg-slate-800 rounded w-96"/>
                    <div class="h-32 bg-slate-800 rounded"/>
                </div>
            }>
                {move || package.get().map(|result| match result {
                    Err(e) => view! {
                        <div class="text-center py-20">
                            <p class="text-red-400 text-lg">"Package not found"</p>
                            <p class="text-slate-500 text-sm mt-2">{e.to_string()}</p>
                        </div>
                    }.into_any(),
                    Ok(pkg) => {
                        let install_cmd = format!("fang install {}", pkg.package_id);
                        let version_display = pkg.latest_version.clone().unwrap_or_else(|| "—".to_string());
                        let repo_url = pkg.repository_url.clone();
                        view! {
                            <div>
                                <div class="mb-8">
                                    <div class="flex items-start justify-between gap-4 mb-3">
                                        <div>
                                            <h1 class="text-3xl font-bold text-slate-100">{pkg.name.clone()}</h1>
                                            <p class="text-slate-500 font-mono text-sm mt-1">{pkg.package_id.clone()}</p>
                                        </div>
                                        <span class="inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-sky-900 text-sky-100 shrink-0">
                                            {format!("v{version_display}")}
                                        </span>
                                    </div>

                                    <p class="text-slate-300 text-lg mb-4">{pkg.description.clone()}</p>

                                    <div class="flex items-center gap-3 flex-wrap text-sm text-slate-500">
                                        <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-slate-800 text-slate-300">
                                            {pkg.category.clone()}
                                        </span>
                                        {pkg.tags.iter().map(|tag| view! {
                                            <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-slate-800 text-slate-300">
                                                {tag.clone()}
                                            </span>
                                        }).collect_view()}
                                        <span>{format!("{} installs", pkg.install_count)}</span>
                                        <span>{format!("by {}", pkg.owner)}</span>
                                    </div>
                                </div>

                                // Install command
                                <div class="bg-slate-900 border border-slate-700 rounded-xl p-5 mb-8">
                                    <h2 class="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-3">"Install"</h2>
                                    <div class="flex items-center gap-3 bg-slate-950 rounded-lg px-4 py-3 font-mono text-sm">
                                        <span class="text-slate-600">"$"</span>
                                        <span class="text-slate-200">{install_cmd}</span>
                                    </div>
                                </div>

                                // Repository link
                                {repo_url.map(|url| view! {
                                    <div class="mb-8">
                                        <a
                                            href=url
                                            class="inline-flex items-center gap-2 text-sky-400 hover:text-sky-300 text-sm transition-colors no-underline"
                                            target="_blank"
                                            rel="noopener noreferrer"
                                        >
                                            <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                                                <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
                                            </svg>
                                            "View Source"
                                        </a>
                                    </div>
                                })}

                                // Version history
                                <div>
                                    <h2 class="text-xl font-semibold text-slate-100 mb-4">"Version History"</h2>
                                    <Suspense fallback=|| view! {
                                        <div class="animate-pulse h-20 bg-slate-900 rounded-xl"/>
                                    }>
                                        {move || versions.get().map(|result| match result {
                                            Ok(vers) => view! {
                                                <div class="space-y-3">
                                                    {vers.into_iter().map(|v| view! {
                                                        <VersionRow version=v/>
                                                    }).collect_view()}
                                                </div>
                                            }.into_any(),
                                            Err(_) => view! { <div/> }.into_any(),
                                        })}
                                    </Suspense>
                                </div>
                            </div>
                        }.into_any()
                    }
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn VersionRow(version: PackageVersion) -> impl IntoView {
    let size_kb = version.archive_size_bytes / 1024;
    view! {
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 flex items-center justify-between gap-4">
            <div class="flex items-center gap-3">
                <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-sky-900 text-sky-100 font-mono">
                    {format!("v{}", version.version)}
                </span>
                <div>
                    <p class="text-sm text-slate-300">
                        {version.release_notes.unwrap_or_else(|| "No release notes".to_string())}
                    </p>
                    <p class="text-xs text-slate-500 mt-0.5">
                        {format!("Published by {} · {} KB", version.published_by, size_kb)}
                    </p>
                </div>
            </div>
            <span class="text-xs text-slate-500 shrink-0">
                {format!("{} installs", version.install_count)}
            </span>
        </div>
    }
}
