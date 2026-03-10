use leptos::prelude::*;

use crate::models::SearchResult;

#[component]
pub fn PackageCard(pkg: SearchResult) -> impl IntoView {
    let href = format!("/packages/{}", pkg.package_id);
    let version_text = pkg.latest_version.clone().unwrap_or_default();
    let has_version = pkg.latest_version.is_some();
    let tags: Vec<String> = pkg.tags.iter().take(2).cloned().collect();
    let install_count = pkg.install_count.to_string();

    view! {
        <a
            href=href
            class="block bg-slate-900 border border-slate-800 rounded-xl p-5 hover:border-slate-600 transition-colors group no-underline"
        >
            <div class="flex items-start justify-between gap-3 mb-2">
                <div class="flex-1 min-w-0">
                    <h3 class="font-semibold text-slate-100 group-hover:text-sky-400 transition-colors truncate">
                        {pkg.name.clone()}
                    </h3>
                    <p class="text-xs text-slate-500 font-mono mt-0.5">
                        {pkg.package_id.clone()}
                    </p>
                </div>
                {move || has_version.then(|| view! {
                    <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-sky-900 text-sky-100 shrink-0">
                        {format!("v{}", version_text.clone())}
                    </span>
                })}
            </div>

            <p class="text-sm text-slate-400 line-clamp-2 mb-3">
                {pkg.description.clone()}
            </p>

            <div class="flex items-center justify-between gap-2 flex-wrap">
                <div class="flex items-center gap-2 flex-wrap">
                    <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-slate-800 text-slate-300">
                        {pkg.category.clone()}
                    </span>
                    {tags.into_iter().map(|tag| view! {
                        <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-slate-800 text-slate-300">
                            {tag}
                        </span>
                    }).collect_view()}
                </div>
                <div class="flex items-center gap-3 text-xs text-slate-500">
                    <span class="flex items-center gap-1">
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                                d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                            />
                        </svg>
                        {install_count}
                    </span>
                </div>
            </div>
        </a>
    }
}
