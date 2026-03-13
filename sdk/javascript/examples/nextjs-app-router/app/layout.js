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

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <head>
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

