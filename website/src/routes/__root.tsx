import {
  createRootRoute,
  HeadContent,
  Outlet,
  Scripts,
  useRouterState,
} from "@tanstack/react-router";
import { Header } from "../components/Header";
import { Footer } from "../components/Footer";
import "../globals.css";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1.0" },
      { title: "TermSurf" },
    ],
  }),
  component: RootComponent,
});

function RootComponent() {
  const { location } = useRouterState();
  const isWelcome = location.pathname === "/welcome";

  return (
    <html lang="en" className="dark">
      <head>
        <HeadContent />
      </head>
      <body className="bg-background text-foreground min-h-screen font-sans">
        {isWelcome ? (
          <Outlet />
        ) : (
          <div className="scanline-overlay max-w-3xl mx-auto px-4 py-6">
            <Header />
            <main>
              <Outlet />
            </main>
            <Footer />
          </div>
        )}
        <Scripts />
      </body>
    </html>
  );
}
