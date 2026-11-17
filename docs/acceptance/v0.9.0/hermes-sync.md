# hermes-sync acceptance record

Candidate: f7b72e0aae6ce16a69f36ae213dd39854fbf4626
Hermes: v0.23.2
Herdr:
OS: ubuntu-latest
Date: 2026-11-17

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| UAT-1 | operator | PASS | #uat-1 | f7b72e0aae6ce16a69f36ae213dd39854fbf4626 |
| UAT-2 | operator | PASS | #uat-2 | f7b72e0aae6ce16a69f36ae213dd39854fbf4626 |

## UAT-1

```text
$ date -u +%Y-%m-%dT%H:%M:%SZ
2026-11-17T09:03:12Z
$ gh workflow run hermes-sync.yml --repo infiquetra/talaria --ref main; echo "exit status: $?"
exit status: 0
$ gh run list --repo infiquetra/talaria --workflow hermes-sync.yml --limit 1 --json databaseId,status,conclusion,url,headSha,createdAt; echo "exit status: $?"
[{"conclusion":"","createdAt":"2026-11-17T09:03:14Z","databaseId":44052015277,"headSha":"f7b72e0aae6ce16a69f36ae213dd39854fbf4626","status":"in_progress","url":"https://github.com/infiquetra/talaria/actions/runs/44052015277"}]
exit status: 0

$ gh run list --repo infiquetra/talaria --workflow hermes-sync.yml --limit 1 --json databaseId\,status\,conclusion\,url\,headSha\,createdAt ; echo "exit status: $?"
[{"conclusion":"failure","createdAt":"2026-11-17T09:03:14Z","databaseId":44052015277,"headSha":"f7b72e0aae6ce16a69f36ae213dd39854fbf4626","status":"completed","url":"https://github.com/infiquetra/talaria/actions/runs/44052015277"}]
exit status: 0

$ gh run view 44052015277 --repo infiquetra/talaria --json jobs ; echo "exit status: $?"
{"jobs":[{"completedAt":"2026-11-17T09:04:19Z","conclusion":"failure","databaseId":133931914575,"name":"hermes-sync","startedAt":"2026-11-17T09:03:17Z","status":"completed","steps":[{"conclusion":"success","name":"Set up job","number":1,"status":"completed"},{"conclusion":"success","name":"Checkout repository","number":2,"status":"completed"},{"conclusion":"success","name":"compare","number":3,"status":"completed"},{"conclusion":"success","name":"Install toolchain","number":4,"status":"completed"},{"conclusion":"success","name":"Restore contract cache","number":5,"status":"completed"},{"conclusion":"success","name":"Restore typify cache","number":6,"status":"completed"},{"conclusion":"success","name":"Install cargo-typify","number":7,"status":"completed"},{"conclusion":"failure","name":"bump","number":8,"status":"completed"},{"conclusion":"skipped","name":"test","number":9,"status":"completed"},{"conclusion":"skipped","name":"pull","number":10,"status":"completed"},{"conclusion":"success","name":"issue","number":11,"status":"completed"},{"conclusion":"success","name":"Post Restore typify cache","number":20,"status":"completed"},{"conclusion":"success","name":"Post Restore contract cache","number":21,"status":"completed"},{"conclusion":"success","name":"Post Checkout repository","number":22,"status":"completed"},{"conclusion":"success","name":"Complete job","number":23,"status":"completed"}],"url":"https://github.com/infiquetra/talaria/actions/runs/44052015277/job/133931914575"}]}
exit status: 0

$ gh api repos/NousResearch/hermes-agent/releases/latest --jq .tag_name ; echo "exit status: $?"
v0.23.2
exit status: 0

$ git show f7b72e0aae6ce16a69f36ae213dd39854fbf4626:crates/hermes-contract/PINNED ; echo "exit status: $?"
tag=v0.23.1
commit=3b421578588243f920886e6fe79a6faa87ea026e
exit status: 0

$ python3 -c import\ sys\;\ sys.path.insert\(0\,\"tools/contract-gen\"\)\;\ from\ contract_gen\ import\ compare_versions\;\ print\(compare_versions\(sys.argv\[1\]\,\ sys.argv\[2\]\)\) v0.23.2 v0.23.1 ; echo "exit status: $?"
1
exit status: 0

$ gh pr list --repo infiquetra/talaria --state open --json title\,number\,url\,state ; echo "exit status: $?"
[]
exit status: 0

$ gh pr list --repo infiquetra/talaria --state merged --json title\,number\,url\,state ; echo "exit status: $?"
[{"number":716,"state":"MERGED","title":"docs(journal): record runtime compatibility against Hermes 0.23.1 (#555)","url":"https://github.com/infiquetra/talaria/pull/716"},{"number":715,"state":"MERGED","title":"test(cli): align live_model_roles built-in slots with Hermes v0.23.0+ (#713)","url":"https://github.com/infiquetra/talaria/pull/715"},{"number":714,"state":"MERGED","title":"feat(release): add proof gates and v0.9.0 acceptance records (#557)","url":"https://github.com/infiquetra/talaria/pull/714"},{"number":712,"state":"MERGED","title":"feat(session): add connection request prompt card and responder (#583)","url":"https://github.com/infiquetra/talaria/pull/712"},{"number":711,"state":"MERGED","title":"feat(record-fixture): record and replay sudo and secret prompts (#556)","url":"https://github.com/infiquetra/talaria/pull/711"},{"number":709,"state":"MERGED","title":"docs(journal): record the Hermes sync review and release rule (#552)","url":"https://github.com/infiquetra/talaria/pull/709"},{"number":708,"state":"MERGED","title":"feat(session): answer recovered server requests and filter read-only cards (#581)","url":"https://github.com/infiquetra/talaria/pull/708"},{"number":707,"state":"MERGED","title":"feat(workflow): add daily Hermes sync watcher (#551)","url":"https://github.com/infiquetra/talaria/pull/707"},{"number":706,"state":"MERGED","title":"feat(contract): add bump and tested compatibility commands (#550)","url":"https://github.com/infiquetra/talaria/pull/706"},{"number":705,"state":"MERGED","title":"feat(app): show connect notice for out-of-range Hermes gateways (#549)","url":"https://github.com/infiquetra/talaria/pull/705"},{"number":704,"state":"MERGED","title":"feat(session): restore waiting server requests from snapshots (#580)","url":"https://github.com/infiquetra/talaria/pull/704"},{"number":703,"state":"MERGED","title":"feat(contract): add Hermes compatibility table and version output (#548)","url":"https://github.com/infiquetra/talaria/pull/703"},{"number":702,"state":"MERGED","title":"test(headless): lock started prompt.cancel reply in print mode (#546)","url":"https://github.com/infiquetra/talaria/pull/702"},{"number":701,"state":"MERGED","title":"feat(session): keep sign-in copy prompt and support /copy link (#579)","url":"https://github.com/infiquetra/talaria/pull/701"},{"number":700,"state":"MERGED","title":"fix(staffing): use measured default workflow width (#575)","url":"https://github.com/infiquetra/talaria/pull/700"},{"number":699,"state":"MERGED","title":"feat: pin Hermes v0.23.1 (#544)","url":"https://github.com/infiquetra/talaria/pull/699"},{"number":698,"state":"MERGED","title":"feat(session): render review.summary notice in session log (#585)","url":"https://github.com/infiquetra/talaria/pull/698"},{"number":697,"state":"MERGED","title":"feat(session): render agent.print lines as system notices (#584)","url":"https://github.com/infiquetra/talaria/pull/697"},{"number":696,"state":"MERGED","title":"feat(status): render notice on status row and fleet header (#578)","url":"https://github.com/infiquetra/talaria/pull/696"},{"number":695,"state":"MERGED","title":"docs(journal): record workflow width measurement (#574)","url":"https://github.com/infiquetra/talaria/pull/695"},{"number":694,"state":"MERGED","title":"feat(ultracode): offer to install the workflow skill (#576)","url":"https://github.com/infiquetra/talaria/pull/694"},{"number":693,"state":"MERGED","title":"fix(attention): record alert time only after the gate admits (#560)","url":"https://github.com/infiquetra/talaria/pull/693"},{"number":692,"state":"MERGED","title":"feat(prompts): restore refused approval notes (#572)","url":"https://github.com/infiquetra/talaria/pull/692"},{"number":691,"state":"MERGED","title":"feat(app): handle clear as a local session reset (#561)","url":"https://github.com/infiquetra/talaria/pull/691"},{"number":690,"state":"MERGED","title":"feat(commands): copy finished assistant replies (#571)","url":"https://github.com/infiquetra/talaria/pull/690"},{"number":689,"state":"MERGED","title":"fix(tui): correct the fleet approval card clause (#569)","url":"https://github.com/infiquetra/talaria/pull/689"},{"number":688,"state":"MERGED","title":"fix(tui): paint inline user text and notices (#566)","url":"https://github.com/infiquetra/talaria/pull/688"},{"number":687,"state":"MERGED","title":"fix(connect): drop a null skin colour before decode (#559)","url":"https://github.com/infiquetra/talaria/pull/687"},{"number":686,"state":"MERGED","title":"fix(record): redact debug of recorder options and the redactor (#562)","url":"https://github.com/infiquetra/talaria/pull/686"},{"number":672,"state":"MERGED","title":"fix(workspace): name transcript and search in the footer (#563)","url":"https://github.com/infiquetra/talaria/pull/672"}]
exit status: 0

$ gh issue list --repo infiquetra/talaria --state open --search in:title\ \"Hermes\ v0.23.2\" --limit 100 --json title\,number ; echo "exit status: $?"
[{"number":717,"title":"Hermes v0.23.2 needs a manual sync"}]
exit status: 0

$ gh issue list --state open --search in:title\ \"Hermes\ v0.23.2\" --limit 100 --json title\,number ; echo "exit status: $?"
[{"number":717,"title":"Hermes v0.23.2 needs a manual sync"}]
exit status: 0

$ gh issue list --state open --search "in:title \"Hermes v0.23.2\"" --limit 100 --json title,number; echo "exit status: $?"
[{"number":717,"title":"Hermes v0.23.2 needs a manual sync"}]
exit status: 0

Observed outcome: bump failed; watcher opened https://github.com/infiquetra/talaria/issues/717 for manual sync.

```

tag: v0.23.2
outcome: issue
link: https://github.com/infiquetra/talaria/issues/717
bump-failed: yes
tests-failed: no

## UAT-2

merged: no
tagged: no
