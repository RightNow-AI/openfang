use leptos::prelude::*;

/// Top-level layout wrapper — renders the navbar, slot for page content, and footer.
#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen flex flex-col">
            <Navbar/>
            <main class="flex-1">
                {children()}
            </main>
            <Footer/>
        </div>
    }
}

#[component]
fn Navbar() -> impl IntoView {
    view! {
        <header class="sticky top-0 z-50 bg-slate-950/90 backdrop-blur border-b border-slate-800">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <div class="flex items-center justify-between h-16 gap-4">
                    <a href="/" class="flex items-center gap-2.5 shrink-0 no-underline">
                        <div class="w-8 h-8 rounded-lg bg-sky-600 flex items-center justify-center text-white font-bold text-sm">
                            "F"
                        </div>
                        <span class="font-semibold text-slate-100 text-lg">"FangHub"</span>
                        <span class="hidden sm:inline text-slate-500 text-sm">"marketplace"</span>
                    </a>

                    <form method="GET" action="/search" class="flex-1 max-w-xl">
                        <div class="relative">
                            <svg
                                class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500"
                                fill="none"
                                stroke="currentColor"
                                viewBox="0 0 24 24"
                            >
                                <path
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    stroke-width="2"
                                    d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                                />
                            </svg>
                            <input
                                type="search"
                                name="q"
                                placeholder="Search Hand packages..."
                                class="w-full bg-slate-900 border border-slate-700 rounded-lg pl-10 pr-4 py-2.5 text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500 focus:border-transparent text-sm"
                            />
                        </div>
                    </form>

                    <nav class="flex items-center gap-3">
                        <a
                            href="/search"
                            class="hidden sm:block text-slate-400 hover:text-slate-100 text-sm transition-colors no-underline"
                        >
                            "Browse"
                        </a>
                        <a
                            href="/api/auth/github"
                            class="inline-flex items-center gap-2 px-4 py-1.5 rounded-lg bg-sky-600 hover:bg-sky-500 text-white font-medium text-xs transition-colors no-underline"
                        >
                            "Sign in with GitHub"
                        </a>
                    </nav>
                </div>
            </div>
        </header>
    }
}

#[component]
fn Footer() -> impl IntoView {
    view! {
        <footer class="border-t border-slate-800 py-8 mt-16">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <div class="flex flex-col sm:flex-row items-center justify-between gap-4 text-sm text-slate-500">
                    <div class="flex items-center gap-2">
                        <div class="w-5 h-5 rounded bg-sky-600 flex items-center justify-center text-white text-xs font-bold">
                            "F"
                        </div>
                        <span>"FangHub v0.3.33 — part of the OpenFang ecosystem"</span>
                    </div>
                    <div class="flex items-center gap-4">
                        <a
                            href="https://github.com/ParadiseAI/maestro-legacy"
                            class="hover:text-slate-300 transition-colors no-underline"
                            target="_blank"
                            rel="noopener noreferrer"
                        >
                            "GitHub"
                        </a>
                        <a href="/search" class="hover:text-slate-300 transition-colors no-underline">
                            "Browse Packages"
                        </a>
                        <a
                            href="https://github.com/ParadiseAI/maestro-legacy/blob/main/docs/fanghub-publishing-guide.md"
                            class="hover:text-slate-300 transition-colors no-underline"
                            target="_blank"
                            rel="noopener noreferrer"
                        >
                            "Publishing Guide"
                        </a>
                    </div>
                </div>
            </div>
        </footer>
    }
}
