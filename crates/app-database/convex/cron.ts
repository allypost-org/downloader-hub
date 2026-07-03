import { cronJobs } from "convex/server";
import { internal } from "./_generated/api";

const crons = cronJobs();

crons.interval(
  "cleanup connections",
  { seconds: 60 },
  internal.connections.cleanup,
);

export default crons;
