# The maintainer's words the first canon rows stand on

Said by the maintainer on 23 November 2026, in answer to questions put to him while the unit, identity and messaging work of the workspace that runs homma was planned. Each section holds one answer verbatim and complete, with the question and its options as they were put, and the intent read from it. The options are the asker's words, shortened; the quote is his. The tenets under `mock/registry/tenet/` cite these sections in their `sources`.

Names of people, identities and repositories in the quotes are the workspace's and stay as he said them. The rows say what is homma's in homma's own terms.

## One name per unit

Question: how the messaging channels are cut. Options: one channel per arc; one per team plus one for all; one per arc plus an announcement channel.

> "Option 1 but perhaps we should do actual unifying here, and tie those to actual projects, and actual state files, so all of a certain unit can be addressed via same name and its cohesive like that"

Intent: one channel per body of work, and that body of work, its project, its state and its channel share one name, so everything of a unit is addressed by it.

## What the concept is called

Question: what the one concept is called. Options: unit, with "project" for a path of one segment and "arc" for a deeper one, as words for depth only; project at every depth; arc at every depth.

> "unit (Recommended)"

Intent: the concept is a unit; project and arc say only how deep a unit sits.

## Whether units nest

Question: does a unit nest. Options: yes, a project and its arcs, `kaski` and `kaski::render`, one channel branch per project with the arcs inside it; no, every arc a flat name with its own branch; only projects are units, an arc staying a team addressed through its hand.

> "Nest (Recommended)"

Intent: units nest by path, a project and its arcs being the same kind of thing at two depths, with one channel branch per project.

## Which states are kept

Question: which of the workspace's state files move under the unified names. Options: the live ones now; all of them; none renamed.

> "We want to be able to deprecate states (projects/arcs) to git history so they remain archived. I think we do an audit of them all and see which ones are stale, deprecated or irrelevant, or unimportant, and retain only those that aren't any of that, then establish a project or an arc for each, depending what the naming will be for our unified concept"

Intent: the tree keeps the state of live units only; a stale, deprecated, irrelevant or unimportant state is removed from the tree and kept in git's history as its archive.

## Dormant units

Question: what happens to three declared units whose state last moved two months earlier. Options: keep the row marked parked and archive the state; keep them as they are; archive the state and drop the row.

> "Those named three are to be continued and I'd like to keep them"

Intent: a unit whose work is to be continued is live, however long it has slept.

## A handle the id grammar cannot spell

Question: how a handle is written inside an id when the path grammar cannot spell it, as with one that opens on a digit. Options: the catalogue row declares a path form beside the handle; the path standard admits an ident that opens on a digit; the handle changes.

> "A check should error on the channel, and config should allow optionally specifying a different channel than the handle is. This way it defaults to the handle without excessive configuration but is overrideable on errors or when wished"

Intent: what is named after a handle defaults to the handle; a check errors where the handle cannot be that name, and configuration may set another.

## Where the identity catalogue lives

Question: where the catalogue of identities lives. Options: a catalogue file of its own beside the character files; the character files' front matter; the `[org.*]` rows of `homma.toml`.

> "Homma toml, one source of truth."

Intent: the identity catalogue is the manifest's identity rows and nothing else; a character file carries prose.

## One identity at a time

Question: what class of tree the workspace repository is. Options: an internal tree; a public-side tree; unchanged.

> "Yeah clause dev is private and we want track record going forward. I propose we use the very same identities the applets already allow importing, I suspect they are the Claude compatible agent files, and if not; something similar. Pretty sure we'll want to compare notes with the old hands design in clause dev and make use of the same idea for future agent identities: each agent picks one identity from the catalogue and one identity can only ever be used once at a time, so they act like they are a single person and when they do one thing in one place, they can't be anywhere else. Humanlike names and personalities will make it easier for us, or me at least, to make sense of the things. Names like "coord" or "rebder-coord" aren't really human friendly. We want stable identities for who is primary coord always though, same as for the other coords. The helpers use the pool of those not in use or reserved for a role then. Thoughts? I think we want the fable to also review and give their proposals and thoughts on this side"

Intent: an agent works under one identity from the catalogue, and an identity is held by one agent at a time, as one person is in one place; a role has a stable identity, and the rest draw from those free.

## A sticky container per identity

Asked unprompted, then answered. Options: sticky per identity; fresh per job; sticky, capped.

> "Okay so I wonder, would the tracks be more sensible if they were set at start of week and were kept in the one container each until onni repoints slots and one is archived as result maybe? Or is it better to get fresh containers for all work? I'm kind of feeling like each identity/person be it helper or coord, gets one sticky container and lane and new work is assigned to them by whoever manages them, instead of each work being its own container. Thoughts?"

> "Sticky per identity (Recommended)"

Intent: an identity keeps one container and one lane, and new work is assigned to it by whoever manages it, rather than each piece of work getting a container of its own.

## Teams

Question: what a lighter helper identity is. Options: journeymen, with sub-agents keeping the plus tag; the coordinator with a plus tag; eight journeymen.

> "Yeah, this makes sense. I would say that we should establish teams so a hand can reserve a couple of Johnny men no journey men for the team for a week and after the weekly wrap up, those are released so this way, the identities, even for the helpers kind of work in one similar domain over a week and it's not as chaotic they should be considered kind of juniors"

Then, unprompted:

> "Also for the teams, other hands can be chosen too, not only journeymen. Perhaps best it's split: hands are opus or fable and journeymen sonnet or haiku. Each hand can be either or, not statically mapped, same for journeymen"

Intent: a hand reserves journeymen, juniors, and other hands for its team for a week, released at the weekly wrap-up, so an identity works in one domain for the week.

## The memory of an identity

Said unprompted.

> "Then also we'll want each identity to, in addition of coord and helper split memories (and those that are for all), also have their own special memory each that they sync with and keep always. And we want to QuickStart each identity with some spoofed past memories according to their personality or such, perhaps the design had that already too in the old one"

Intent: every identity keeps a memory of its own, synced and kept always, beside the memories split by position and those for all, and starts with a past written to its personality.

## Where a channel lives

Question: where a channel lives. Options: a `sauna/<channel>` branch in the workspace repository; keep `coord/<channel>`; a repository of its own.

> "sauna/c/<ch>"

Intent: a channel is the branch `sauna/c/<channel>`.

## Git as the truth of the channels

Said unprompted, on the messaging tool.

> "we can make improvements to existing msg that optionally uses the workers we have to speed things up or asynchronously manage the channels and such while source of truth always remains the git and a worker can be shut down and things still work via git in its absence, though slower or not as nice. And a worker can be added and dropped at will and everything syncs and stops and resumes seamlessly from/to the git state. The handles is what pings should default to, names, nick or proper, only used in prose if at all. Depends on personality I suppose. We'll want to improve the way attachments and such work too, so they don't just have a parallel place outside of the repos in question that should get reports in their mockspace for example, which had before been a bit of a problem. It needs a smart way to integrate with mockspaces and homma itself I think, and reuse as much as it can from them. Homma primarily in general and mocksoace as extra synergy level if one exists where worked."

Intent: git is the only source of truth of the channels, and a worker that speeds them up can be added or dropped at any time with everything resuming from git; a ping addresses a handle, and names are for prose; a report or an attachment lands in the repository it concerns, through homma first and its mockspace where there is one, never in a store beside the repositories.

## Addressing

Question: who may message whom. Options: anyone, but a message across teams does not wake; anyone, and every message wakes; feudal, through one's lead.

> "Option 3, feudal, is the etiquette. But option 2 is the de facto protocol, though discouraged to break retiquette"

Intent: anyone may message anyone and every message wakes its addressee; going through one's lead is the etiquette, and breaking it is discouraged.

## Trust in the sender

Question: how far the sender of a message must be trusted. Options: advisory now, signed once a secrets store exists; advisory only; signed from the start.

> "Sign from the start"

Intent: every message is signed by its identity's key from the first landing of the format.

## A parked identity

Question: what reaches an identity with no work this hour. Options: its hand's fire only; an hourly task for every identity; a six-hourly task while parked.

> "Hands fire only and ONLY if they remain active and needed. Let's say that after third consequent "nothing to do" wake, a hand parks and DMs coord to wake them by ping when they are needed again."

Intent: an identity is woken only while it is active and needed; after the third wake in a row with nothing to do it parks and messages its lead, who wakes it by a ping when it is needed again.

## Hierarchical ids

Said unprompted.

> "Also queue a piece of work we'll have to do before work starts: creating actual hierarchical ids / slugs for all tickets and such. They are long messes of underscores in prose now. We might want to start namespacing like ticket::question::engine::internals::deps::allowed. Then querying for ticket::question::engine gives all engine related tickets. Querying ::internals then gives those. We migrate all domain or such fields into slugs using any namespaces after their schema kind"

Intent: an id is a path, the schema kind first, then its namespaces, then a short leaf; a query by a prefix lists everything under it and a query by an inner segment finds it at any depth.

## Ids are letkajenkka paths

Said unprompted, on the hierarchical ids.

> "Remember that everything there is letkajenkka spec, so querying the hierarchical namespaces/nodes/segments is also globbable and conforms to the spec so things like inline arrays or whatever syntax in letkajenkka will work on them too."

Intent: the ids are letkajenkka paths, and querying them is a letkajenkka query, globs and every other selector of the standard included.

## Where the tickets live

Said unprompted.

> "Another thing to finish before real work but after the v2 etc bigger work: remove agenda from clause-dev in full and initialize and start using the homma impl of the mockspace-registry on any homma content repo instead."

Intent: a homma content repository keeps its tickets and rows in homma's implementation of the mockspace registry, and the tracker that stood beside it goes.

## One framework

Said unprompted.

> "In fact homma should also adopt down the line the mocksoace tools framework for its tools, and pick and impl into itself the other related things that are going to be similar to what mockspace does. Like the agent/ pipeline. Which means a lot of the things homma does better, are ported upstream, such as the concept of elaboration and rule cards and whatever else. Homma workspaces then also gain lints and rust support for tools. And upstream we want mockspace to support fully nutshell or posix shell tools and lints, not only rust. So we unify the two and both use the mockspace standards and we consolidate them so we pick the best parts of each and share the crates. We then also get much less duplication and similar disjoint code and maintainability hell, when a fix on mockspace lints also reaches homma. And a fix made on homma's hook framework extends to improve all mockspaces too. And when the two conform fully, no need to conjure up complex separate paths for mockspace child repos, just use the one and same path and abstractions and cli verbs and behaviour from workspace to mockspace to project etc."

Intent: homma and mockspace become one framework on shared crates, each taking what the other does better, so a workspace, a mockspace and a project use the same paths, abstractions, verbs and behaviour, and a fix in either reaches both.

## No forge feature

Question: what guards the boundary of a repository. Options: a stamp hook with a periodic audit; a paid plan with the forge's rulesets; a guard only once the repository is public.

> "Let's not depend on forge features at all, public or no. No reliance on any GitHub features anywhere in our pipeline. Even the PRs for dev-main should be described generally to fit any forge or git host procedure and fall back to our own internal pr flow using same machinery as promotions."

Intent: nothing in the pipeline relies on a forge's own features; the move from `dev` to `main` is described for any git host and runs on our own flow, a forge's pull request being at most a mirror of it.

## A repository is self-contained

Question: how homma's canon speaks, given that homma is public and the workspace's names and the maintainer's words are not. Options: two tiers, neutral rows in homma and the workspace's own rows in the workspace's registry; neutral rows citing the workspace by reference; rows quoting inline, the registry hidden first.

> "They can't be referencing workspace from the repo. There's a seam that isn't bidirectional. The repo can't contain refs that can't be resolved within the repo, same as symlinks. And for mockspace stuff, it'll be tucked away into its own custom refs, before this work ever starts, so nothing named should remain in the public end, only the harness and abstract/general use homma stuff as intended"

Then, unprompted, on the reading of that answer as a rule for homma alone:

> "That isn't just a special rule for homma. Each repo is self contained unless by nature it is a workspace like clause-dev and even there, we'd rather generalise than hard code assumptions of what is contained or available"

Intent: every repository is self-contained: it holds no reference, citation, path or link that cannot be resolved inside it, as no symlink may point out of it. The seam runs one way. Only a repository that is by nature a workspace reaches across, and even a workspace generalises what it finds instead of hard-coding which repositories or files are there.
