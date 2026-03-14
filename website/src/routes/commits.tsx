import { createFileRoute } from "@tanstack/react-router";
import { CommitLog } from "../components/CommitLog";
import commitsData from "../../data/commits.json";

export const Route = createFileRoute("/commits")({
  head: () => ({ meta: [{ title: "Commits — TermSurf" }] }),
  component: CommitsPage,
});

function CommitsPage() {
  return <CommitLog commits={commitsData.commits} />;
}
