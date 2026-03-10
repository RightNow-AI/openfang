use leptos::prelude::*;

use crate::models::HandPackage;

/// Server function — fetches packages owned by the authenticated user.
#[server(GetMyPackages, "/api/leptos")]
pub async fn get_my_packages() -> Result<Vec<HandPackage>, ServerFnError> {
    use crate::auth::extract_user_from_request;
    use crate::store::RegistryStore;
    use leptos_axum::extract;
    use std::sync::Arc;

    let store: axum::extract::Extension<Arc<RegistryStore>> = extract().await?;
    let user = extract_user_from_request()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    store
        .get_packages_by_owner(&user.github_login)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    let my_packages = Resource::new(|| (), |_| get_my_packages());

    view! {
        <div class="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <div class="flex items-center justify-between mb-8">
                <div>
                    <h1 class="text-2xl font-bold text-slate-100">"My Packages"</h1>
                    <p class="text-slate-500 text-sm mt-1">"Manage your published Hand packages"</p>
                </div>
                <a
                    href="https://github.com/ParadiseAI/maestro-legacy/blob/main/docs/fanghub-publishing-guide.md"
                    class="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white font-medium text-sm transition-colors"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    "Publish a Package"
                </a>
            </div>

            <Suspense fallback=|| view! {
                <div class="space-y-3">
                    {(0..3).map(|_| view! {
                        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 h-20 animate-pulse"/>
                    }).collect_view()}
                </div>
            }>
                {move || my_packages.get().map(|result| match result {
                    Err(_) => view! {
                        <div class="text-center py-16 text-slate-500">
                            <p class="text-lg mb-2">"Not signed in"</p>
                            <p class="text-sm">
                                <a href="/api/auth/github" class="text-sky-400 hover:underline">
                                    "Sign in with GitHub"
                                </a>
                                " to manage your packages"
                            </p>
                        </div>
                    }.into_any(),
                    Ok(pkgs) if pkgs.is_empty() => view! {
                        <div class="text-center py-16 text-slate-500">
                            <p class="text-lg mb-2">"No packages yet"</p>
                            <p class="text-sm">
                                "Use "
                                <code class="font-mono text-slate-300">"fang publish"</code>
                                " to publish your first Hand package"
                            </p>
                        </div>
                    }.into_any(),
                    Ok(pkgs) => view! {
                        <div class="space-y-3">
                            {pkgs.into_iter().map(|pkg| {
                                let href = format!("/packages/{}", pkg.package_id);
                                let version = pkg.latest_version.clone().unwrap_or_else(|| "—".to_string());
                                view! {
                                    <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 flex items-center justify-between gap-4">
                                        <div>
                                            <a href=href class="font-semibold text-slate-100 hover:text-sky-400 transition-colors no-underline">
                                                {pkg.name.clone()}
                                            </a>
                                            <p class="text-xs text-slate-500 font-mono mt-0.5">{pkg.package_id.clone()}</p>
                                        </div>
                                        <div class="flex items-center gap-4 text-sm text-slate-500">
                                            <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-sky-900 text-sky-100">
                                                {format!("v{version}")}
                                            </span>
                                            <span>{format!("{} installs", pkg.install_count)}</span>
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}
