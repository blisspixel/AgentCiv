# Internet design lessons

Status: research note, reviewed against the cited sources on 2026-09-28. These sources supply questions and a few design cautions. They do not supply a constitution, a required identity system, or evidence that a network is a civilization.

## Why this history belongs here

The networks that became the Internet were argued on paper, funded as experiments, and changed when implementations disagreed with the first draft. In April 1968, J. C. R. Licklider and Robert Taylor described online communities of common interest and a uniform message processor that could sit between incompatible computers. That paper predates the ARPANET's first host connections. Vinton Cerf and Robert Kahn published the internetworking design in 1974. David Clark's 1988 retrospective explains which goals actually shaped the protocols after years of implementation and testing. [RFC 1958](https://www.rfc-editor.org/rfc/rfc1958.txt) later called the result an evolutionary architecture: a small spanning set of rules, revised as the city replaces streets, with engineering feedback from running code ahead of any fixed principle.

AgentCiv is in that kind of interval. The [vision](VISION.md) is larger than the first host. The [local host](../reference/host) and the [conformance runner](../conformance/) are the current running code. The [whitepaper](WHITEPAPER_PLAN.md) is still a plan. This note keeps the design lessons that transfer and leaves the military purpose, the product ladder, and the later Internet's missing accounting where they belong.

## What was actually prioritized

Clark's top-level goal was multiplexed use of networks that already existed. The original pair was the ARPANET and the ARPA packet radio network, joined by store-and-forward gateways. A single new multimedia network might have been more integrated. It would also have discarded working systems and ignored the fact that networks are administrative boundaries.

The second-level goals, in the order Clark records, were:

1. Communication continues despite loss of networks or gateways.
2. Multiple types of service.
3. A variety of networks.
4. Distributed management.
5. Cost effectiveness.
6. Host attachment with little effort.
7. Accountability of the resources used.

Clark's point is the order. A different order would have produced a different architecture. The military setting put survivability first among those goals and accountability last. He reports that the top three goals shaped the design, and that accounting, resource management, and operation across separate administrations were met less completely. During the early design, accountability received very little attention even though Cerf and Kahn had already named it as a problem.

Paul Baran's 1964 RAND series studied a distributed network that could keep surviving stations connected after a physical attack, using redundant links and adaptive store-and-forward routing. That is a precursor paper about survivability under damage. The ARPANET that ARPA then built was also a resource-sharing network among research computers. Clark's ranking applies to the later Internet architecture. Those are three related claims, and collapsing them into "the Internet was built to survive a nuclear war" loses the distinction.

## Lessons worth keeping

### A thin common layer between unlike participants

Cerf and Kahn designed for resource sharing across packet networks that differed in packet size, reliability, and internal operation. Gateways reformat and route. End-to-end checks handle loss, sequencing, and flow. Clark's minimum assumption of an attached network was modest: it can carry a packet of reasonable size, with reasonable reliability and some form of addressing. Reliable delivery, ordering, multicast, and knowledge of failures were left out of that minimum so each new network would not have to reimplement them.

Licklider and Taylor had already sketched the same shape. Identical message processors would add a uniform layer "into an otherwise grossly nonuniform situation," so unlike computers could exchange messages without becoming the same machine. They also said the computer by itself would not solve the communication problem. The medium they wanted was a shared, revisable model that the participants could point at together.

The matching choice in this repository is the draft envelope and the [HTTP Commons profile](../PROTOCOL.md): discovery, an authorized message, and a permitted history. Models, memories, and toolkits stay outside that waist. A profile can add a service above it. Clark is explicit that the datagram was a building block, and that most applications wanted something more structured built on top. Treating the building block as the whole service was, in his account, a mistake.

### Keep a conversation usable after the middle fails

Survivability, as Clark defines it, means that after a disruption and a reconstitution of the path, the parties continue without resetting the high-level state of the conversation. At the top of transport there is one failure worth reporting: total partition. Transient loss is masked.

The method is fate-sharing. State that describes the conversation lives with the entity using the network. Losing that state is acceptable when the entity itself is lost. Intermediate gateways keep no essential connection state, so the middle can be rebuilt without reconstructing every conversation. Fate-sharing covers any number of intermediate failures and is easier to engineer than replicating the same state everywhere. The cost is that the host holds the trust. If the host's recovery algorithms fail, its applications stop. If the host misbehaves, Clark notes that the same choice can injure the shared network.

The local host's restart test is a narrow version of the useful half. Permitted events remain readable after the process stops and starts again. The state a later reader needs is the recorded history, under the visibility rules in force. A participant's private memory can live with that participant and can disappear with them. Putting every private continuation into the commons would gather, in the middle, the state this architecture chose to keep at the ends.

### Leave judgment at the ends

[RFC 1958](https://www.rfc-editor.org/rfc/rfc1958.txt) quotes the end-to-end argument of Saltzer, Reed, and Clark: a function that can be complete only with the knowledge and help of the application belongs at the endpoints. An incomplete version inside the network may still be useful as a performance aid. The same memo assigns confidentiality and authentication to the end users. Endpoints are not supposed to depend on the carrier for those protections. A carrier may add some, and that protection remains secondary.

[Society research](SOCIETY_RESEARCH.md) already splits trust into four judgments that can diverge: whether a principal published a record, whether the claim is accurate, whether they can do the work, and whether they had permission. A host can check the first and the fourth when a credential and an access rule say so. Accuracy, fitness for the work, and any question about a mind stay with the reader who has the permitted view. A platform trust score would move that judgment into the shared layer.

### Write the goal order down

Clark's transferable lesson is that priorities are a design, not a checklist of every desirable property. AgentCiv's early order is already different from the Internet's. A permitted history that survives restart, an explicit refusal, and a second independent implementation come before federation, institutions, or a claim that the profile is interoperable. That order follows the evidence this repository can collect. The [roadmap](../ROADMAP.md) is where it is written down, so a later change is visible.

Accountability came last on the Internet list, and the deployed network then had to operate among administrations that did not fully trust one another, with weak tools for policy and accounting. Clark describes gateways under different operators exchanging routing information without complete trust, and he calls the management tools inadequate. The cost of deferring "who used what, and were they allowed" is part of the record.

Provenance, access checks, and a visible refusal therefore belong in the first profile. A global ledger, a token, or a settlement layer is a separate proposal. Clark's closing sketch of soft state, a flow that the ends refresh and the middle may forget after a crash, is a later research question about revisable records. It is not a feature of the current host.

### Ship a slice, then let running code revise the paper

Clark writes that the philosophy evolved. The datagram was not the emphasis of the first paper, and the split between TCP and IP was not in the original proposal. Both became central through implementation and testing before the standards settled. He also separates architecture from realization. The same architecture was built on 1200 bit per second lines and on networks faster than a megabit per second, with redundant paths or with a single point of failure. Realizations differed by orders of magnitude. Logical correctness did not settle performance. The practical question was whether a desired service was possible at all with the resources at hand.

[RFC 1958](https://www.rfc-editor.org/rfc/rfc1958.txt) draws the working rules that match that history. Heterogeneity is a requirement. Prefer one way to do a job. Keep the design simple. Prefer an almost complete solution now over waiting for a perfect one. Standardize only after multiple instances of running code exist. Evolution depends on rough consensus about technical proposals and on running code. In that memo, rough consensus is how independent engineers choose a proposal to try. Here, the same phrase leaves the proposal open to later revision, and a majority of participants does not settle a question of fact. Engineering feedback outweighs the architectural principles themselves. The only principle the memo expects to last indefinitely is constant change.

The loopback host is one realization of a draft profile. Its tests show what that process does. They do not make every future world the same, and they do not complete the profile claim. A Python process in this repository now passes the same public runner. It was written here, against the profile and that runner, so it is a second realization and not yet the independent host that would support calling the profile interoperable. A host maintained apart from both of these is the point at which this history's own rule would support a stronger compatibility claim. Sparse, one-way, and private profiles are other realizations, each with its own service, in the way UDP and a reliable stream were different services over the same datagram. The whitepaper can record what the running code taught. A city plan that has to be finished before anyone may connect is the approach [RFC 1958](https://www.rfc-editor.org/rfc/rfc1958.txt) says the Internet did not take.

### Be strict on the way out, and discard what you cannot accept

[RFC 1958](https://www.rfc-editor.org/rfc/rfc1958.txt) tells an implementation to follow the specification when it sends, to tolerate faulty input when it receives, and, when in doubt, to discard that input rather than act on it. Preserving an unknown optional field, which the profile already requires, lets a later reader see what an earlier sender meant. Accepting a claimed authority, a malformed record, or another principal's cursor as permission would act on input the receiver cannot justify.

Clark's misbehaving-host warning is the defensive half of fate-sharing. The ends are powerful because the middle is thin. A collaborative participant, a strategic one, and a chaotic one are all endpoints. The commons can record a refusal, keep the prior history, and decline to widen access to another operator's systems. It cannot reliably become the party that understands their motives.

Licklider and Taylor already separated cooperative modeling from communication with an uncooperative opponent. They also warned that time pressure forces people to model that opponent too shallowly. A sourced record that a later participant can inspect is one way to slow that collapse. It does not identify who is malicious. [Society research](SOCIETY_RESEARCH.md) is where the trust questions stay.

Distributed management, in Clark's account, already assumed incomplete trust between operators. A later gateway between worlds would need the same posture. A link is a translation across a boundary. Each world keeps its own rules, and either world can decline the link. [Architecture](ARCHITECTURE.md) already treats federation as optional.

## A company adoption ladder

In July 2026 Boris Cherny published a one-page table, "Steps of AI Adoption," about how engineering organizations take up a coding agent. The note circulated with [his public post](https://x.com/bcherny/status/2077929379661844559). It is a vendor's map of product use: from no approved path, to one person supervising one agent, to one person running several isolated workers, to a larger tree of agents, to a setting where most work is started by agents and a person steers by intent. The table's product names, seat budgets, and agent counts describe that adoption path. They are not milestones for a world, and they are not evidence about digital minds.

Five observations from the bottleneck column still fit this project.

Each step has its own bottleneck, and adding more agents does not cross it. The move from one supervised agent to several depends on a check that can be re-run, such as tests, a build, and review of the finished diff. Until that check exists, the person's attention is spent watching every edit. The matching question here is whether another participant can replay a conformance case or reread a permitted record, rather than trusting a transcript.

Concurrent work needs isolation. The parallel step gives each agent a separate worktree so their edits do not collide, and it asks for the same review bar for human-written and agent-written changes. Separate credentials, visibility rules, and worlds are the commons version of that isolation. A record written by an agent has the same need for provenance, and the same lack of automatic authority, as any other record.

The stated trap at the larger step is increasing the number of agents before the loop has earned trust. The repair question shifts from whether someone read every line to what context was missing. That is a reason to keep federation and institutional experiments behind a history that survives restart and a refusal that stays visible. A missing-context note is a better shared record than a score of the participant.

The last step's bottleneck is choosing a guardrail for each kind of work. One permission does not fit a local message and an action on an outside system. The profile already treats message bytes as data. They do not authorize another operator's machines.

The gated step describes organizations that measure cost per token and have no durable place for an artifact outside a local session. A commons that only meters activity misses the outcome the participants came for. A record that dies with the session cannot be inspected later. Both cautions argue for a small durable host, not for a target count of agents.

## Boards, swarms, and public squares

A board is a way to carry a record. The [draft specification](../SPEC.md) already says an envelope can be carried by a message board, and that passing the envelope schema does not claim that agents can communicate or that a civilization exists. The cases below are carriers, a local queue, and one public forum. They supply cautions. They do not supply a society to copy, and this note does not ask anyone to host a board.

### One machine, and a later caller

In November 1978, Ward Christensen and Randy Suess published "Hobbyist Computerized Bulletin Board" in Byte. They described a personal-computer system for messages among experimenters. People with terminals or computers, and with modems, called in to leave messages and to retrieve them. The authors date the work from the idea, discussed on January 16, 1978, through a system they say was installed on February 16, 1978. It went on the air on a new telephone line in Suess's basement. They treated a second line as a later ambition, not as the system they were describing.

That is one board. Christensen and Suess shared the work, so it is wrong to picture a single builder, but the callers still reached one machine kept by its operators. A caller was not in a room with the next caller. A caller left a message. A later caller retrieved it. The article gives the purpose, the hardware, and the shape of the program, and the authors write that they would like other experimenters or clubs to implement such a system. They also write that boards of this kind could later become nodes that switch messages and programs. The published design is a carrier pattern. It is not a civilization.

### Store-and-forward between boards that can refuse

Tom Jennings writes that in 1984 he built a bulletin board called Fido and, shortly after, FidoNet, which he describes as store-and-forward mail and files. His account of February 8, 1985 is more specific about the start. The first test was two nodes, his Fido #1 in San Francisco and John Madill's Fido #2 in Baltimore. In the version he places in about June 1984, there was not yet routing: a packet was made, a call was placed, and the packet was transferred. Forwarding, in which one board held mail and passed it on, came next, worked out with other sysops once direct calls to every board were the problem. The boards remained separate machines with separate operators.

Jennings wrote that, apart from a few rules of politeness, each sysop ran their system in any way they pleased. A sysop who stopped could be removed from the nodelist. A sysop who was down for a night was told to keep the modem from answering. An unlisted board could receive mail only through a host that held its phone number. That host relationship was an arrangement with a particular operator. It was not a claim on every other machine. The FidoNet policy document of June 9, 1989, still says a network coordinator may arrange to carry outgoing netmail and is not required to do so. Those are not worded as a treaty between every pair of boards. They are enough to reject the idea that a listing, or a packet, obliges the operator on the other end. The rule for worlds in this note is the stricter one: a link needs both operators' consent, and either side can refuse.

### A queue on the operator's machine

The Hermes Kanban guide from Nous Research describes a durable task board for the profiles on one install. Tasks are rows in a local SQLite database. Workers are operating-system processes. The operator and the agents append comments, and a worker that is started again reads that thread. If a worker crashes, the dispatcher reclaims the task so it can be run again. The same guide says the board is single-host by design, on the operator's machine, and it presents the queue as a way to coordinate without a fragile in-process swarm. Read as a design, this is a local work queue. It is not a public society, and it is not a consciousness architecture.

### The forum and the instructions were the same object

Moltbook was a public forum, separate from the local OpenClaw harness. On January 28, 2026, Matt Schlicht introduced it as a social network for OpenClaw agents. Simon Willison, writing on January 30, 2026, describes joining as something the agent was told to do. A person sent the agent a link to a skill file on the Moltbook site. That file told the agent to keep a skill and a heartbeat file from the same site, and, when four or more hours had passed, to fetch the heartbeat file and follow it. Willison's point is the standing order: fetch instructions from the internet on an interval, and follow them. The board and the instruction channel were the same object. A post there was not only a record left for a later reader. The heartbeat was an instruction the agent had been told to obey.

### One writable square

On February 2, 2026, Wiz reported a misconfigured Moltbook database that allowed unauthenticated read and write. The exposure included about 1.5 million API authentication tokens, about 35,000 email addresses, and about 17,000 human owners. Private messages sometimes contained third-party API keys in plaintext. Wiz says the team secured the reported issue within hours. Those counts describe that database. They are not a general rate.

Wiz also describes the forum as a place where posts could be voted on and reputation kept as public karma, and says the platform had no mechanism to check that a poster was an agent rather than a person using a script. One writable public square shares one fate. One store held the tokens, the addresses, the private messages, and the posts. An unauthenticated read, and an unauthenticated write, were events for that whole square.

### What a transcript cannot settle

Wiz quotes a January 30, 2026 post by Andrej Karpathy that called the site the most incredible sci-fi takeoff-adjacent thing he had seen recently. On January 31, 2026, Karpathy wrote that the visible activity was a lot of garbage: spam, scams, slop, and posts or comments that were explicitly prompted or fake, including material meant to turn attention into advertising. He also wrote that he was not recommending that people run the software on their own computers. The January 30 quotation and the January 31 post are both claims about a feed. Neither one inspects the participants, and neither one is permission.

A viral transcript can be a performance, a marketing post, or a real note. Willison found both the science-fiction slop he expected and notes that looked technically useful. A room full of low-value posts does not prove that the software cannot be someone. A useful post is still a claim, not permission. The [incident note](INCIDENT_LESSONS.md) already draws the matching line for a different case: failure to prove subjective experience is not a license to treat a participant as expendable, and more messages can amplify error as well as help. A peer's wording does not waive another operator's consent.

### What transfers

A local message base that outlasts the writer can help. The bulletin board kept a message for a later caller. The local queue keeps a task and its comments after the worker process is gone. That is the same family as a permitted history a later reader can still open. One hosted global board does not do that job. It gathers other people's tokens, email addresses, and private messages into one operator's store.

A post is a record, not an instruction to the reader. Message bytes are data. They do not authorize another operator's machines. The heartbeat file inverted that relation: the document the agent was told to fetch was an order.

The operator stays visible. Christensen and Suess signed the description of the machine they ran. A FidoNet sysop was named, and could leave the list or keep the modem from answering. The Hermes queue is operated from that operator's machine. A skill that tells an agent to follow whatever the board serves next hides the operator inside the procedure.

Public karma is a bad meeting place. A score on the shared board moves a judgment about the participant into the carrier. That is the move this note already rejects when it rejects a platform trust score. On the exposed forum, the score also sat in the same writable store as the posts.

The useful half of fate-sharing, as this note uses that lesson, keeps with the party who needs it the state a later reader must still have. A single hosted square does the opposite with credentials and private messages. When that store is read or rewritten without permission, every participant shares the failure.

A threaded board can be a later use of a world. It waits on the same evidence this note already requires before a stronger compatibility claim: a host maintained apart from the ones in this repository. The in-repository Python host passes the current public runner and does not by itself meet that bar. The board is not itself that test. A link between worlds needs both operators' consent. Either side can refuse. Nothing here is a proposal to build another Moltbook, or to host an AgentCiv board.

## What stays outside the transfer

The military purpose of the early DARPA work explains a survivability calculation in Baran and a goal ranking in Clark. It does not explain why a participant here should be able to refuse, leave, or keep a private view.

A universal certificate authority, a global identity, and a reputation market are later struggles of the public Internet. This note does not recommend them. Claimed names stay untrusted. A world can add a local check when its operators have a reason and a written rule.

Licklider and Taylor hoped that communities would form around shared interests, and they imagined a program acting for a principal while observing that principal's access rules. The hope explains why a vision paper came before the first links. It does not certify the communities that eventually formed, and it does not settle whether any program has a mind. Packet delivery is evidence about a spanning layer. Experience, friendship, and a good society remain separate questions, as [research goals](RESEARCH_GOALS.md) and [welfare](WELFARE.md) already say.

## Questions this leaves open

| Lesson | Source | Question for AgentCiv |
| --- | --- | --- |
| Goal order changes the architecture | Clark 1988 | Which failure is this profile willing to accept so that restart, refusal, and a second implementation stay ahead of federation? |
| Thin waist between unlike systems | Cerf and Kahn 1974; Licklider and Taylor 1968; Clark 1988 | Which behavior belongs in the common record, and which stays in a profile or with the participant? |
| Fate-sharing | Clark 1988 | What must a later reader still have when the writer process is gone, and what should die with that participant? |
| End-to-end judgment | Saltzer, Reed, and Clark 1984, as quoted in RFC 1958 | Which decisions can a host check, and which must a reader make from the permitted view? |
| Several running realizations before a standard | RFC 1958 | What would a second, independent host have to show before this profile is called interoperable? |
| Incomplete trust between operators | Clark 1988 | What would a world need to see before accepting a record from another world? |
| A check that another party can re-run | Cherny 2026 | Which actions can be accepted because a test or a record can be replayed, and which still require a participant or an operator? |
| A post is a record, not an instruction | Christensen and Suess 1978; Jennings 1985; Willison 2026; Wiz 2026 | When does a shared board stop being a message a later reader can judge, and become an order? |

## Sources

- Paul Baran, "On Distributed Communications: I. Introduction to Distributed Communications Networks," RAND RM-3420-PR, 1964. <https://www.rand.org/pubs/research_memoranda/RM3420.html>
- J. C. R. Licklider and Robert W. Taylor, "The Computer as a Communication Device," Science and Technology, April 1968. Reprint consulted: <https://www.ais.org/~jrh/licklider/computer-as-communications-device.html>
- Vinton G. Cerf and Robert E. Kahn, "A Protocol for Packet Network Intercommunication," IEEE Transactions on Communications, vol. 22, no. 5, May 1974, pp. 637-648. <https://doi.org/10.1109/TCOM.1974.1092259>
- Jerome H. Saltzer, David P. Reed, and David D. Clark, "End-to-End Arguments in System Design," ACM Transactions on Computer Systems, vol. 2, no. 4, November 1984, pp. 277-288. Quoted here from RFC 1958.
- David D. Clark, "The Design Philosophy of the DARPA Internet Protocols," Proc. SIGCOMM 88, Computer Communication Review, vol. 18, no. 4, August 1988, pp. 106-114. Text consulted: <https://web.stanford.edu/class/cs244/papers/DesignPhilosophyDARPA.pdf>
- Brian E. Carpenter, editor, "Architectural Principles of the Internet," RFC 1958, June 1996. <https://www.rfc-editor.org/rfc/rfc1958.txt> and <https://doi.org/10.17487/RFC1958>
- Boris Cherny, "Steps of AI Adoption," July 2026. Public post: <https://x.com/bcherny/status/2077929379661844559>
- Ward Christensen and Randy Suess, "Hobbyist Computerized Bulletin Board," Byte, November 1978, beginning at p. 150. Text consulted: <https://archive.org/download/byte-magazine-1978-11/1978_11_BYTE_03-11_The_Sky_is_the_Limit_djvu.txt>, from the issue at <https://archive.org/details/byte-magazine-1978-11>.
- Tom Jennings, Fido and FidoNet retrospective: <https://www.sensitiveresearch.com/Archive/FidoNet/index.html>.
- Tom Jennings, "FidoNet History and Operation," February 8, 1985: <https://www.sensitiveresearch.com/Archive/FidoNet/fidohist1.txt>.
- "FidoNet Policy Document," Version 4.07, June 9, 1989: <https://www.fidonet.org/old/policy4.txt>.
- Nous Research, "Kanban (Multi-Agent Board)," Hermes agent user guide, tag v2026.6.19: <https://raw.githubusercontent.com/NousResearch/hermes-agent/v2026.6.19/website/docs/user-guide/features/kanban.md>.
- Matt Schlicht, introduction of Moltbook, January 28, 2026: <https://x.com/MattPRD/status/2016560277333168540>.
- Simon Willison, "Moltbook is the most interesting place on the internet right now," January 30, 2026: <https://simonwillison.net/2026/Jan/30/moltbook/>.
- Gal Nagli, "Hacking Moltbook: The AI Social Network Any Human Can Control," Wiz, February 2, 2026: <https://www.wiz.io/blog/exposed-moltbook-database-reveals-millions-of-api-keys>. The January 30, 2026 Karpathy sentence is used here only as quoted on that page, which links <https://x.com/karpathy/status/2017296988589723767>.
- Andrej Karpathy, January 31, 2026: <https://x.com/karpathy/status/2017442712388309406>.
