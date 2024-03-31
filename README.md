<div align="center">

*Brought to you by*

  <a href="https://wizardsardine.com" target="_blank">
    <img src="ws_logo.png" width="400px" />
  </a>

</div>

# Ledger Installer

Setup your Ledger signing device without having to use Ledger Live.

**WARNING: this is a very rudimentary proof of concept.**

## Why?

Ledger makes great hardware. However their software is lacking. Nowadays to simply setup your Ledger
signing device for use with your favourite Bitcoin wallet you have to navigate through all the ads
and **scams** (which Ledger Live recklessly nudges you toward).  Recently they even turned on the
subscription to Ledger Recover by default. That is, when guiding a user through setting up their
signing device they would nudge him to sign up to Ledger recover (thereby directly losing
self-custody) as part of his "backup" process.

Having to deal with Ledger Live has been a recurring pain point for our users (for
[Liana](https://github.com/wizardsardine/liana)). These irresponsible practices have pushed me to
investigate offering an option to our users, and all bitcoiners, to setup a Ledger without having to
use Ledger Live. This PoC is the first step toward this goal.

Of course, Ledger [does not want to document their
protocol](https://x.com/achow101/status/1773333790389055848) so i had to go through Ledger Live's
confusing Javascript codebase to "reverse engineer" the parts i was interested in. Credits to Ava
Chow for an [earlier
investigation](https://gist.github.com/achow101/3604bf50aa622b33ad2160cc77075a8c) focused on
upgrading the firmware which i could take inspiration from.

## Usage

**This is a PoC. Use at your own risk.**

For now this is a simple command line tool which can talk to a Ledger device connected by USB. The
commands are communicated using an environment variable, `LEDGER_COMMAND`. Another env var lets you
switch to testnet (for instance to install the test app), simply set `LEDGER_TESTNET`.

For now those commands are implemented:
- `getinfo`: get information (such as the list of installed apps) for your device
- `genuinecheck`: check your Ledger device is genuine
- `installapp`: install the Bitcoin app on your device
- `openapp`: open the Bitcoin app on your device

### Examples

#### Checking your Ledger is genuine

```
LEDGER_COMMAND=genuinecheck cargo run
```
```
Querying Ledger's remote HSM to perform the genuine check. You might have to confirm the operation on your device.
Success. Your Ledger is genuine.
```

#### Installing the Bitcoin Test app on your Ledger

```
LEDGER_TESTNET=1 LEDGER_COMMAND=installapp cargo run
```
```
Querying installed applications from your Ledger. You might have to confirm on your device.
Querying Ledger's remote HSM to install the app. You might have to confirm the operation on your device.
Successfully installed the app.
```

## Future

First of all we are now going to investigate pulling bits of this PoC into [Liana](https://github.com/wizardsardine/liana).

Also now that the main mechanisms are in place it should be fairly straightforward to implement the
missing features. What i'd like to see:
- An `updateapp` command
- An `upgradefirmware` command

Also, it would be nice to have a tiny Iced GUI for this project. If someone pulls this off, we could
polish it a little and start distributing binaries for anyone to benefit, not only Liana users and
people who can use the command line.

Contributions welcome!

NOTE: i am not interested in supporting altcoins. If you want to add support for one, feel free to
fork the project.
