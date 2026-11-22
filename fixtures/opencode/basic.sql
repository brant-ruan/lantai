-- Synthetic OpenCode 1.x database (shape of OpenCode 1.18.35, 2026-11). Hand-written, no real session content.
-- Tables keep the real column layout of the four tables the adapter reads; other tables are omitted.
-- Times are Unix milliseconds. 1795000000000 = 2026-11-18T11:06:40Z.

CREATE TABLE `project` (
  `id` text PRIMARY KEY, `worktree` text NOT NULL, `vcs` text, `name` text, `icon_url` text,
  `icon_url_override` text, `icon_color` text, `time_created` integer NOT NULL, `time_updated` integer NOT NULL,
  `time_initialized` integer, `sandboxes` text NOT NULL, `commands` text
);
CREATE TABLE `session` (
  `id` text PRIMARY KEY, `project_id` text NOT NULL, `workspace_id` text, `parent_id` text, `slug` text NOT NULL,
  `directory` text NOT NULL, `path` text, `title` text NOT NULL, `version` text NOT NULL, `share_url` text,
  `summary_additions` integer, `summary_deletions` integer, `summary_files` integer, `summary_diffs` text,
  `metadata` text, `cost` real DEFAULT 0 NOT NULL, `tokens_input` integer DEFAULT 0 NOT NULL,
  `tokens_output` integer DEFAULT 0 NOT NULL, `tokens_reasoning` integer DEFAULT 0 NOT NULL,
  `tokens_cache_read` integer DEFAULT 0 NOT NULL, `tokens_cache_write` integer DEFAULT 0 NOT NULL,
  `revert` text, `permission` text, `agent` text, `model` text, `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL, `time_compacting` integer, `time_archived` integer
);
CREATE TABLE `message` (
  `id` text PRIMARY KEY, `session_id` text NOT NULL, `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL, `data` text NOT NULL
);
CREATE TABLE `part` (
  `id` text PRIMARY KEY, `message_id` text NOT NULL, `session_id` text NOT NULL, `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL, `data` text NOT NULL
);

INSERT INTO project VALUES ('prj_app', '/Users/u/pjts/app', 'git', NULL, NULL, NULL, NULL, 1795000000000, 1795000000000, NULL, '[]', NULL);
INSERT INTO project VALUES ('global', '/', NULL, NULL, NULL, NULL, NULL, 1795000000000, 1795000000000, NULL, '[]', NULL);

-- Session 1: a normal session in a repository subdirectory, covering every part and tool shape.
INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated)
VALUES ('ses_0001', 'prj_app', NULL, 'brave-otter', '/Users/u/pjts/app/server', 'Fix login redirect', '1.18.35', 1795000000000, 1795000600000);

-- Turn 1: typed text, a synthetic expansion (skipped) and an image.
INSERT INTO message VALUES ('msg_0001', 'ses_0001', 1795000000000, 1795000000000,
  '{"role":"user","time":{"created":1795000000000},"agent":"build","model":{"providerID":"gateway","modelID":"model-a"},"summary":{"diffs":[]}}');
INSERT INTO part VALUES ('prt_0001', 'msg_0001', 'ses_0001', 1795000000000, 1795000000000,
  '{"type":"text","text":"  Fix the login redirect loop  "}');
INSERT INTO part VALUES ('prt_0002', 'msg_0001', 'ses_0001', 1795000000000, 1795000000000,
  '{"type":"text","text":"Called the Read tool with the following input: {}","synthetic":true}');
INSERT INTO part VALUES ('prt_0003', 'msg_0001', 'ses_0001', 1795000000000, 1795000000000,
  '{"type":"file","mime":"image/png","filename":"shot.png","url":"data:image/png;base64,AAAA"}');
INSERT INTO part VALUES ('prt_0004', 'msg_0001', 'ses_0001', 1795000000000, 1795000000000,
  '{"type":"file","mime":"text/plain","filename":"notes.txt","url":"file:///Users/u/notes.txt"}');

-- Reply 1: reasoning, text, and tools (completed, failing exit code, error status, running).
INSERT INTO message VALUES ('msg_0002', 'ses_0001', 1795000001000, 1795000060000,
  '{"role":"assistant","parentID":"msg_0001","mode":"build","agent":"build","path":{"cwd":"/Users/u/pjts/app/server","root":"/Users/u/pjts/app"},"cost":0,"tokens":{"total":1660,"input":100,"output":50,"reasoning":10,"cache":{"read":1500,"write":0}},"modelID":"model-a","providerID":"gateway","time":{"created":1795000001000,"completed":1795000060000},"finish":"tool-calls"}');
INSERT INTO part VALUES ('prt_0010', 'msg_0002', 'ses_0001', 1795000001000, 1795000001000, '{"type":"step-start","snapshot":"abc"}');
INSERT INTO part VALUES ('prt_0011', 'msg_0002', 'ses_0001', 1795000001000, 1795000002000,
  '{"type":"reasoning","text":"thinking","time":{"start":1795000001000,"end":1795000002000}}');
INSERT INTO part VALUES ('prt_0012', 'msg_0002', 'ses_0001', 1795000002000, 1795000003000,
  '{"type":"text","text":"Let me look at the router.","time":{"start":1795000002000,"end":1795000003000}}');
INSERT INTO part VALUES ('prt_0013', 'msg_0002', 'ses_0001', 1795000003000, 1795000004000,
  '{"type":"tool","tool":"read","callID":"call_read","state":{"status":"completed","input":{"filePath":"/Users/u/pjts/app/server/router.ts"},"output":"<file>...</file>","metadata":{"preview":"","truncated":false},"title":"router.ts","time":{"start":1795000003000,"end":1795000004000}}}');
INSERT INTO part VALUES ('prt_0014', 'msg_0002', 'ses_0001', 1795000004000, 1795000005000,
  '{"type":"tool","tool":"bash","callID":"call_test","state":{"status":"completed","input":{"command":"npm test","description":"Run tests","workdir":"/Users/u/pjts/app/server"},"output":"1 failing\nexpected 302","metadata":{"output":"","exit":1,"truncated":false},"title":"npm test","time":{"start":1795000004000,"end":1795000005000}}}');
INSERT INTO part VALUES ('prt_0015', 'msg_0002', 'ses_0001', 1795000005000, 1795000006000,
  '{"type":"tool","tool":"bash","callID":"call_ls","state":{"status":"completed","input":{"command":"ls","description":"List files"},"output":"a\nb","metadata":{"exit":0},"time":{"start":1795000005000,"end":1795000006000}}}');
INSERT INTO part VALUES ('prt_0016', 'msg_0002', 'ses_0001', 1795000006000, 1795000007000,
  '{"type":"tool","tool":"webfetch","callID":"call_fetch","state":{"status":"error","input":{"url":"https://example.com/docs","format":"markdown"},"error":"Request failed with status 404","time":{"start":1795000006000,"end":1795000007000}}}');
INSERT INTO part VALUES ('prt_0017', 'msg_0002', 'ses_0001', 1795000007000, 1795000008000,
  '{"type":"tool","tool":"edit","callID":"call_edit","state":{"status":"completed","input":{"filePath":"/Users/u/pjts/app/server/router.ts","oldString":"a\nb","newString":"a\nc\nd"},"output":"","metadata":{"diff":"","truncated":false},"time":{"start":1795000007000,"end":1795000008000}}}');
INSERT INTO part VALUES ('prt_0018', 'msg_0002', 'ses_0001', 1795000008000, 1795000009000,
  '{"type":"tool","tool":"write","callID":"call_write","state":{"status":"completed","input":{"filePath":"/Users/u/pjts/app/server/redirect.test.ts","content":"one\ntwo\nthree"},"output":"","metadata":{"exists":false},"time":{"start":1795000008000,"end":1795000009000}}}');
INSERT INTO part VALUES ('prt_0019', 'msg_0002', 'ses_0001', 1795000009000, 1795000010000,
  '{"type":"tool","tool":"apply_patch","callID":"call_patch","state":{"status":"completed","input":{"patchText":"*** Begin Patch\n*** Update File: server/auth.ts\n@@\n-old\n+new\n*** Add File: server/CHANGES.md\n+note\n+more\n*** End Patch"},"output":"done","metadata":{},"time":{"start":1795000009000,"end":1795000010000}}}');
INSERT INTO part VALUES ('prt_0020', 'msg_0002', 'ses_0001', 1795000010000, 1795000011000,
  '{"type":"tool","tool":"task","callID":"call_task","state":{"status":"completed","input":{"description":"Search callers","prompt":"find callers","subagent_type":"general"},"output":"found 3","metadata":{"sessionId":"ses_0003"},"time":{"start":1795000010000,"end":1795000011000}}}');
INSERT INTO part VALUES ('prt_0021', 'msg_0002', 'ses_0001', 1795000011000, 1795000012000,
  '{"type":"tool","tool":"glob","callID":"call_glob","state":{"status":"completed","input":{"pattern":"**/*.ts","path":"/Users/u/pjts/app"},"output":"","metadata":{"count":3},"time":{"start":1795000011000,"end":1795000012000}}}');
INSERT INTO part VALUES ('prt_0022', 'msg_0002', 'ses_0001', 1795000012000, 1795000013000,
  '{"type":"tool","tool":"skill","callID":"call_skill","state":{"status":"completed","input":{"name":"review"},"output":"","metadata":{},"time":{"start":1795000012000,"end":1795000013000}}}');
INSERT INTO part VALUES ('prt_0023', 'msg_0002', 'ses_0001', 1795000013000, 1795000014000,
  '{"type":"tool","tool":"question","callID":"call_question","state":{"status":"completed","input":{"questions":[]},"output":"yes","metadata":{"answers":[]},"time":{"start":1795000013000,"end":1795000014000}}}');
INSERT INTO part VALUES ('prt_0024', 'msg_0002', 'ses_0001', 1795000014000, 1795000014000, '{"type":"mystery-part"}');
INSERT INTO part VALUES ('prt_0025', 'msg_0002', 'ses_0001', 1795000014000, 1795000014000, '{"type":"text", broken');
INSERT INTO part VALUES ('prt_0026', 'msg_0002', 'ses_0001', 1795000059000, 1795000060000,
  '{"type":"step-finish","reason":"tool-calls","tokens":{"total":1660,"input":100,"output":50,"reasoning":10,"cache":{"read":1500,"write":0}},"cost":0}');

-- Turn 2 with an aborted reply (error, partial text, no usage) and a reply still streaming.
INSERT INTO message VALUES ('msg_0003', 'ses_0001', 1795000100000, 1795000100000,
  '{"role":"user","time":{"created":1795000100000},"agent":"build","model":{"providerID":"gateway","modelID":"model-a"}}');
INSERT INTO part VALUES ('prt_0030', 'msg_0003', 'ses_0001', 1795000100000, 1795000100000, '{"type":"text","text":"Now update the docs"}');
INSERT INTO message VALUES ('msg_0004', 'ses_0001', 1795000101000, 1795000102000,
  '{"role":"assistant","parentID":"msg_0003","mode":"build","tokens":{"total":0,"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"modelID":"model-a","providerID":"gateway","time":{"created":1795000101000,"completed":1795000102000},"error":{"name":"MessageAbortedError","data":{"message":"Aborted"}}}');
INSERT INTO part VALUES ('prt_0040', 'msg_0004', 'ses_0001', 1795000101000, 1795000102000,
  '{"type":"text","text":"Updating the README","time":{"start":1795000101000}}');
INSERT INTO message VALUES ('msg_0005', 'ses_0001', 1795000200000, 1795000600000,
  '{"role":"assistant","parentID":"msg_0003","mode":"build","tokens":{"total":0,"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"modelID":"model-a","providerID":"gateway","time":{"created":1795000200000}}');
INSERT INTO part VALUES ('prt_0050', 'msg_0005', 'ses_0001', 1795000200000, 1795000600000,
  '{"type":"text","text":"Half a sent","time":{"start":1795000200000}}');
INSERT INTO part VALUES ('prt_0051', 'msg_0005', 'ses_0001', 1795000200000, 1795000600000,
  '{"type":"tool","tool":"bash","callID":"call_running","state":{"status":"running","input":{"command":"npm run build"},"time":{"start":1795000200000}}}');

-- Compaction: a user message with only a compaction marker and a summary reply; neither is new work.
INSERT INTO message VALUES ('msg_0006', 'ses_0001', 1795000300000, 1795000300000,
  '{"role":"user","time":{"created":1795000300000},"agent":"build"}');
INSERT INTO part VALUES ('prt_0060', 'msg_0006', 'ses_0001', 1795000300000, 1795000300000, '{"type":"compaction","auto":false}');
INSERT INTO message VALUES ('msg_0007', 'ses_0001', 1795000301000, 1795000302000,
  '{"role":"assistant","parentID":"msg_0006","mode":"compaction","summary":true,"tokens":{"total":20,"input":10,"output":10,"reasoning":0,"cache":{"read":0,"write":0}},"modelID":"model-a","providerID":"gateway","time":{"created":1795000301000,"completed":1795000302000},"finish":"stop"}');
INSERT INTO part VALUES ('prt_0070', 'msg_0007', 'ses_0001', 1795000301000, 1795000302000,
  '{"type":"text","text":"Summary of the conversation so far","time":{"start":1795000301000,"end":1795000302000}}');

-- Session 2: default placeholder title, outside any repository.
INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated)
VALUES ('ses_0002', 'global', NULL, 'calm-fox', '/Users/u/scratch', 'New session - 2026-11-18T08:30:00.000Z', '1.18.33', 1795000000000, 1795000005000);
INSERT INTO message VALUES ('msg_0100', 'ses_0002', 1795000001000, 1795000001000, '{"role":"user","time":{"created":1795000001000}}');
INSERT INTO part VALUES ('prt_0100', 'msg_0100', 'ses_0002', 1795000001000, 1795000001000, '{"type":"text","text":"hi"}');
INSERT INTO message VALUES ('msg_0101', 'ses_0002', 1795000002000, 1795000005000,
  '{"role":"assistant","tokens":{"total":40,"input":30,"output":5,"reasoning":5,"cache":{"read":0,"write":0}},"modelID":"model-a","providerID":"gateway","time":{"created":1795000002000,"completed":1795000005000},"finish":"stop"}');
INSERT INTO part VALUES ('prt_0101', 'msg_0101', 'ses_0002', 1795000002000, 1795000005000, '{"type":"text","text":"Hello.","time":{"start":1795000002000,"end":1795000005000}}');

-- Session 3: the subagent session created by the task tool; it produces nothing.
INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated)
VALUES ('ses_0003', 'prj_app', 'ses_0001', 'quick-elk', '/Users/u/pjts/app/server', 'Search callers (@general subagent)', '1.18.35', 1795000010000, 1795000011000);
INSERT INTO message VALUES ('msg_0200', 'ses_0003', 1795000010000, 1795000010000, '{"role":"user","time":{"created":1795000010000}}');
INSERT INTO part VALUES ('prt_0200', 'msg_0200', 'ses_0003', 1795000010000, 1795000010000, '{"type":"text","text":"find callers"}');
