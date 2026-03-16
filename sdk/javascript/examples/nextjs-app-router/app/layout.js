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
  body{margin:0}
  .app-shell{display:flex;min-height:100vh}
  .sidebar{flex-shrink:0;width:220px;overflow:hidden}
  .main-content{flex:1;overflow:auto}
  .nav-icon svg{display:block;width:16px;height:16px}
`;

export default function RootLayout({ children }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        {/* eslint-disable-next-line react/no-danger */}
        <style dangerouslySetInnerHTML={{ __html: CRITICAL_CSS }} />
        {/* eslint-disable-next-line react/no-danger */}
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

