import { describe, expect, it } from "vitest";
import { demoPlan } from "./demo";

describe("demo planner data", () => {
  it("draws the graph from the same round as the matrix", () => {
    for (const scenario of ["default", "infeasible", "dense", "long"] as const) {
      const plan = demoPlan(scenario);
      const tracks = new Map(plan.matrix.map((track) => [track.trackCode, track]));
      for (const edge of plan.graph.edges) {
        const cell = tracks.get(edge.trackCode)?.cells[plan.graph.round];
        expect(cell?.participantName, `${scenario} ${edge.trackCode}`).toBe(
          edge.participantName,
        );
      }
    }
  });
});
