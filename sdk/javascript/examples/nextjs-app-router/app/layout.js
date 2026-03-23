import "./globals.css";
import Sidebar from "./components/Sidebar";

export const metadata = {
  title: "OpenFang",
  description: "Agent Operating System",
};

// Inline theme script — runs before first paint to prevent dark/light flash.
// Reads localStorage key set by Sidebar.js (same key as Alpine app).
const THEME_SCRIPT = `(function(){
  try{
    var m=localStorage.getItem('openfang-theme-mode')||'system';
    var dark=m==='dark'||(m!=='light'&&window.matchMedia('(prefers-color-scheme:dark)').matches);
    document.documentElement.setAttribute('data-theme',dark?'dark':'light');
  }catch(e){}
})();`;

// Critical-path CSS loaded synchronously in <head> — prevents layout jumps
// while the main CSS bundle loads.  Only structural rules that are immediately
// visible above the fold; everything else stays in globals.css.
const CRITICAL_CSS = `
  *,*::before,*::after{box-sizing:border-box}
  html,body{margin:0;min-height:100vh;font-family:'Inter',-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;background:#0f172a;color:#f8fafc}
  :root[data-theme='dark']{--bg:#0f172a;--bg-elevated:#131d32;--bg-card:#1e293b;--bg-sidebar:#0b1120;--surface:#1e293b;--surface2:#162032;--surface3:#0f1827;--text:#f8fafc;--text-secondary:#cbd5e1;--text-soft:#94a3b8;--text-dim:#64748b;--text-muted:#64748b;--border:#334155;--border-subtle:rgba(148,163,184,0.07);--border-light:rgba(148,163,184,0.18);--accent:#f97316;--accent-dim:#ea6c0c;--accent-subtle:rgba(249,115,22,0.10);--accent-glow:rgba(249,115,22,0.22);--success:#22c55e;--warning:#f59e0b;--error:#f87171;--sidebar-bg:#0b1120;--sidebar-nav-hover-bg:rgba(248,250,252,0.05);--sidebar-nav-active-bg:rgba(249,115,22,0.08)}
  :root[data-theme='light']{--bg:#f7f4ee;--bg-elevated:#fffdf9;--bg-card:#fffdf9;--bg-sidebar:#ece6dc;--surface:#fffdf9;--surface2:#faf7f1;--surface3:#ece6dc;--text:#16120d;--text-secondary:#5f564a;--text-soft:#5f564a;--text-dim:#7a705f;--text-muted:#7a705f;--border:#d8cfc1;--border-subtle:rgba(23,19,15,0.06);--border-light:rgba(23,19,15,0.14);--accent:#f47a20;--accent-dim:#bc520b;--accent-subtle:rgba(244,122,32,0.08);--accent-glow:rgba(244,122,32,0.18);--success:#1f8f55;--warning:#b7791f;--error:#c2412d;--sidebar-bg:#ece6dc;--sidebar-nav-hover-bg:rgba(255,241,228,0.8);--sidebar-nav-active-bg:rgba(244,122,32,0.08)}
  .app-shell{display:flex;min-height:100vh}
  .sidebar{flex-shrink:0;width:200px;overflow:hidden;background:var(--sidebar-bg);border-right:1px solid var(--border);display:flex;flex-direction:column;color:var(--text-secondary)}
  .sidebar-header{display:flex;align-items:center;gap:8px;padding:12px 12px 10px;border-bottom:1px solid var(--border)}
  .sidebar-logo{width:26px;height:26px;border-radius:5px;background:var(--accent);color:#fff;font-size:11px;font-weight:800;display:flex;align-items:center;justify-content:center;flex-shrink:0}
  .sidebar-status{padding:6px 12px;border-bottom:1px solid var(--border-subtle);display:flex;align-items:center;gap:6px;font-size:10px}
  .conn-dot{width:7px;height:7px;border-radius:999px;background:var(--success);flex-shrink:0}.conn-dot.offline{background:var(--error)}.conn-dot.connecting{background:var(--warning)}
  .sidebar-nav{flex:1;overflow:auto;padding:8px 0}.nav-section{padding-bottom:4px}.nav-section+.nav-section{border-top:1px solid var(--border-subtle);margin-top:4px;padding-top:4px}
  .nav-section-title{display:flex;align-items:center;justify-content:space-between;padding:8px 10px 2px;font-size:9px;font-weight:700;text-transform:uppercase;letter-spacing:.9px;color:var(--text-dim)}
  .nav-item{display:flex;align-items:center;gap:8px;padding:6px 10px;border-radius:6px;margin:1px 6px;font-size:12px;color:var(--text-soft);text-decoration:none;white-space:nowrap;overflow:hidden}
  .nav-item:hover{background:var(--sidebar-nav-hover-bg);color:var(--text)}
  .nav-item.active{background:var(--sidebar-nav-active-bg);color:var(--accent);font-weight:600;box-shadow:inset 2px 0 0 var(--accent)}
  .nav-icon{display:flex;align-items:center;justify-content:center;width:16px;height:16px;flex-shrink:0;opacity:.8}
  .sidebar-footer{padding:10px 12px 12px;border-top:1px solid var(--border)}
  .sidebar-toggle{position:absolute;right:-12px;top:50%;transform:translateY(-50%);width:22px;height:22px;background:var(--bg-elevated);border:1px solid var(--border);border-radius:999px;display:flex;align-items:center;justify-content:center;font-size:10px;color:var(--text-dim)}
  .main-content{flex:1;overflow:auto;background:var(--bg);color:var(--text);padding:14px 20px}
  .page-header{display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;padding-bottom:12px;margin-bottom:14px;border-bottom:1px solid var(--border)}
  .page-header h1,.page-header h2{margin:0;font-size:17px;font-weight:700;letter-spacing:-.4px;color:var(--text)}
  .page-body{max-width:1200px}.loading-state,.error-state,.empty-state{display:flex;align-items:center;justify-content:center;gap:10px;padding:48px 32px;color:var(--text-dim);font-size:13px}.spinner{width:18px;height:18px;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%;animation:spin .7s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
  .nav-icon svg{display:block;width:15px;height:15px}
`;

export default function RootLayout({ children }) {
  return (
    <html lang="en" data-theme="dark" suppressHydrationWarning>
      <head>
        <style dangerouslySetInnerHTML={{ __html: CRITICAL_CSS }} />
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body>
        <div className="app-shell">
          <Sidebar />
          <main className="main-content">{children}</main>
        </div>
      </body>
    </html>
  );
}

