import { Link, useRouterState } from "@tanstack/react-router";
import { $icon } from "../util/icons";

const NAV_ITEMS = [
  { to: "/blog", label: "Blog" },
  { to: "/commits", label: "Commits" },
  { to: "/docs", label: "Docs" },
  { to: "/about", label: "About" },
] as const;

export function Header() {
  const { location } = useRouterState();

  return (
    <header className="mb-8">
      <div className="flex items-center justify-between text-sm">
        <Link to="/" className="flex items-center gap-2 text-primary font-bold">
          <img
            src={$icon("/images/termsurf-11-transparent-192.png")}
            alt="TermSurf logo"
            className="w-6 h-6"
          />
          TermSurf
        </Link>
        <nav className="flex gap-1">
          {NAV_ITEMS.map(({ to, label }) => {
            const active = location.pathname === to;
            return (
              <Link
                key={to}
                to={to}
                className={
                  active
                    ? "text-primary"
                    : "text-muted hover:text-accent"
                }
              >
                {active ? `>[${label}]` : `[${label}]`}
              </Link>
            );
          })}
        </nav>
      </div>
      <div className="mt-3 text-muted text-xs">
        ────────────────────────────────────────────────────────────────────
      </div>
    </header>
  );
}
