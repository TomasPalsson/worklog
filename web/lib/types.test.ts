import { describe, expect, it } from "bun:test";
import { sourceKind } from "./types";
import type {
  LabelOrigin,
  LabelRequest,
  RoutedEvent,
  Rule,
  RoutingStatus,
  RuleKind,
} from "./types";

describe("sourceKind", () => {
  it("recognises github sources", () => {
    expect(sourceKind("github_commit")).toBe("github");
    expect(sourceKind("github_pr")).toBe("github");
  });

  it("recognises claude sources", () => {
    expect(sourceKind("claude_prompt")).toBe("claude");
    expect(sourceKind("claude_session_end")).toBe("claude");
  });

  it("recognises calendar sources (both spellings)", () => {
    expect(sourceKind("gcal_event")).toBe("gcal");
    expect(sourceKind("google_calendar")).toBe("gcal");
  });

  it("falls back to other for unknown sources", () => {
    expect(sourceKind("linear")).toBe("other");
    expect(sourceKind("")).toBe("other");
  });
});

describe("routing types", () => {
  it("RoutedEvent mirrors routing_contract::RoutedEvent field-for-field", () => {
    const event: RoutedEvent = {
      id: 42,
      source: "firefox",
      started_at: "2026-04-14T10:00:00Z",
      title: "AWS docs",
      details: "https://aws.tomasari.is/",
      container: "work",
      folder: null,
      label_origin: null,
      label_confidence: null,
    };
    expect(event.folder).toBeNull();
    expect(event.label_origin).toBeNull();
  });

  it("accepts every value the LabelOrigin and RuleKind Rust enums serialize", () => {
    const origins: LabelOrigin[] = ["rule", "fix", "guess"];
    const kinds: RuleKind[] = ["domain", "slack_channel", "container"];
    expect(origins).toEqual(["rule", "fix", "guess"]);
    expect(kinds).toEqual(["domain", "slack_channel", "container"]);
  });

  it("Rule and LabelRequest share field names with the daemon routes", () => {
    const rule: Rule = {
      id: 1,
      kind: "domain",
      pattern: "aws.tomasari.is",
      folder: "aws-cert",
      created_at: "2026-04-14T10:00:00Z",
    };
    const req: LabelRequest = { folder: "aws-cert", always: "domain" };
    expect(req.always).toBe(rule.kind);
    expect(req.folder).toBe(rule.folder);
  });

  it("RoutingStatus mirrors GET /routing/status", () => {
    const status: RoutingStatus = {
      last_heartbeat: null,
      last_slack: "2026-04-14T10:00:00Z",
      laya_reachable: false,
    };
    expect(status.laya_reachable).toBe(false);
  });
});
