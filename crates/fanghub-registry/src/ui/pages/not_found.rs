use leptos::prelude::*;

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div class="min-h-[60vh] flex items-center justify-center">
            <div class="text-center">
                <div class="text-8xl font-bold text-slate-800 mb-4">"404"</div>
                <h1 class="text-2xl font-semibold text-slate-300 mb-2">"Page not found"</h1>
                <p class="text-slate-500 mb-8">"The page you're looking for doesn't exist."</p>
                <a
                    href="/"
                    class="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white font-medium text-sm transition-colors no-underline"
                >
                    "\u{2190} Back to FangHub"
                </a>
            </div>
        </div>
    }
}
