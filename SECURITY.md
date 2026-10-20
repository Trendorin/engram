# Security policy

Engram reads and writes the memory of other processes, and its engine may hold
`CAP_SYS_PTRACE`. Security problems matter, so please report them privately.

## Reporting

Use GitHub's private vulnerability reporting:
[Report a vulnerability](https://github.com/Trendorin/engram/security/advisories/new).
Please don't open a public issue for a security problem.

Include what an attacker could do, how to reproduce it, and the output of `engram doctor`.
You will get an answer within a week.

## What counts

Examples of what we want to hear about:

- another local user, or a process of another user, can talk to `engramd` or make it act on
  their behalf;
- `engramd` can be made to attach to processes of other users or root;
- a client can change memory, freeze values, pause the target, set watchpoints or start
  programs without approval while the policy is *ask*;
- the installer or `engram uninstall` touches files outside what it installed;
- the capability granted by `engram setup` can be abused beyond reading and writing your own
  processes.

Out of scope: an agent you allowed (*Allow all* or *Allow for session*) doing what it was
allowed to do, and anti-cheat systems detecting Engram.

## Supported versions

Security fixes go into the latest release.
