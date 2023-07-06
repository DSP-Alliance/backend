# FIP Voting Backend Server

This project includes a backend server that implements a delegated voting system for the Ethereum network. It enables Filecoin miners to delegate their voting power to a chosen Ethereum address, following a verification process to ensure the miner controls the miner key.
Table of Contents

- [FIP Voting Backend Server](#fip-voting-backend-server)
  - [Getting Started](#getting-started)
    - [Registration](#registration)
  - [Vote Casting](#vote-casting)

## Getting Started

### Registration

In order to participate in voting, you first need to register by delegating your Filecoin miner's voting power to an Ethereum address. You will need to sign a message with your miner key to achieve this, as well as sign the registration with your worker address. Our backend server will then validate that your worker address controls the specified miner key.

For example, if you register miner f0123 which has a storage power of 2TB, the delegated Ethereum address will possess this total voting power.
Script Execution

To register, you will need to execute a script on your miner computer. You can find this script at ./scripts/registration.bash.

To make this registration process easier, you can use a one-liner bash command to fetch and execute the script:

```bash
curl -sSL https://raw.githubusercontent.com/team-telnyx/fip-voting/master/scripts/registration.bash
```

Once the script is available on your machine, you can run it using the following format:

```bash
./registration.bash <Your_Ethereum_Address> <Miner_ID_1> <Miner_ID_2> ...
```

Here, <Your_Ethereum_Address> should be replaced with your Ethereum address, and <Miner_ID_1>, <Miner_ID_2>, etc. with the IDs of the miners you wish to register. You can register multiple miner IDs by appending them to the end of the script line.

## Vote Casting

To cast a vote, please visit [voting app](fip-voting.vercel.app). Before you can vote, please ensure that you have completed the registration process described above.

In addition, please note that voting can only be initiated by an authorized vote starter who has previously registered their address. Each Ethereum address you delegated during registration is allowed one vote.
Pre-requisites

* Access to your miner computer to run the bash script.

* Control over your miner key and worker address for registration.

* An Ethereum address to delegate your voting power to.
