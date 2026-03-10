/// FangHub UI — Leptos SSR application served from the same Axum binary as the REST API.
///
/// Architecture:
/// - The Axum router serves `/api/*` routes for the REST API.
/// - The Leptos SSR handler serves all other routes as server-rendered HTML.
/// - Client-side hydration is handled by the WASM bundle (built separately with Trunk).
///
/// This module declares the Leptos `App` component and all pages/components.
/// The `leptos_axum::LeptosRoutes` extractor wires them into the Axum router in `server.rs`.

pub mod components;
pub mod pages;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use self::pages::{DashboardPage, HomePage, NotFoundPage, PackagePage, SearchPage};

/// Root Leptos application component.
#[component]
pub fn FangHubApp() -> impl IntoView {
    provide_meta_context();

    view! {
        // Inject global CSS and meta tags
        <Stylesheet id="fanghub-styles" href="/pkg/fanghub.css"/>
        <Title text="FangHub — Hand Package Marketplace"/>

        <Router>
            <main>
                <Routes fallback=|| view! { <NotFoundPage/> }>
                    <Route path=path!("/") view=HomePage/>
                    <Route path=path!("/search") view=SearchPage/>
                    <Route path=path!("/packages/:package_id") view=PackagePage/>
                    <Route path=path!("/dashboard") view=DashboardPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// Shell HTML wrapper for SSR — injects the hydration script and CSS link.
/// This is used by `leptos_axum::render_app_to_stream` to produce the full HTML document.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="description" content="FangHub — the marketplace for OpenFang Hand packages"/>
                <link rel="preconnect" href="https://fonts.googleapis.com"/>
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin=""/>
                <link
                    href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500&family=Inter:wght@300;400;500;600;700&display=swap"
                    rel="stylesheet"
                />
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options.clone()/>
                <MetaTags/>
            </head>
            <body class="bg-slate-950 text-slate-100 antialiased">
                <FangHubApp/>
            </body>
        </html>
    }
}
