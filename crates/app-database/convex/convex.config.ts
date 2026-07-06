import { defineApp } from "convex/server";
import aggregate from "@convex-dev/aggregate/convex.config.js";

const app = defineApp();

// Namespaced aggregate backing `requests:counts` — see convex/lib/requestCounts.ts.
app.use(aggregate, { name: "requestCounts" });

export default app;
