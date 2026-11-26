import { describe, expect, it } from "vitest";
import { demoOwnerId, demoPlan } from "./demo";

describe("demo planner data", () => {
  it("keeps the demo matrix a partial Latin rectangle", () => {
    for (const scenario of ["default", "infeasible", "dense", "long"] as const) {
      const plan = demoPlan(scenario);
      const seen = new Set<string>();
      const roundPeople = new Map<number, Set<string>>();
      const roundTracks = new Map<number, Set<string>>();
      for (const track of plan.matrix) {
        const owner = demoOwnerId(track.trackCode);
        expect(owner, `${scenario} ${track.trackCode} owner`).not.toBeNull();
        for (const cell of track.cells) {
          expect(
            cell.participantId,
            `${scenario} ${track.trackCode} own track in round ${cell.round}`,
          ).not.toBe(owner);
          const pair = `${cell.trackCode}\u0000${cell.participantId}`;
          expect(seen.has(pair), `${scenario} repeated ${pair}`).toBe(false);
          seen.add(pair);
          const people = roundPeople.get(cell.round) ?? new Set<string>();
          people.add(cell.participantId);
          roundPeople.set(cell.round, people);
          const tracks = roundTracks.get(cell.round) ?? new Set<string>();
          tracks.add(cell.trackCode);
          roundTracks.set(cell.round, tracks);
        }
      }
      for (const [round, people] of roundPeople) {
        expect(people.size, `${scenario} round ${round} people`).toBe(
          plan.participants,
        );
        expect(roundTracks.get(round)?.size, `${scenario} round ${round} tracks`).toBe(
          plan.tracks,
        );
      }
    }
  });

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
