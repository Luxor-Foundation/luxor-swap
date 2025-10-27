1. Purpose of the Security Policy

The purpose of this Security Policy is to define the principles and responsibilities that
ensure the security, integrity, and transparency of the Luxor Network ecosystem.
It aims to protect users, their assets, and the smart contracts that power the Luxor
staking and swap dApps on the Solana blockchain.

Luxor Network is committed to maintaining a safe and trustworthy environment for all
participants by implementing clear procedures for vulnerability reporting, risk
mitigation, and continuous monitoring of on-chain activities.

This document serves as a guideline for both users and security researchers to
understand how the Luxor system handles potential security issues and ensures the
long-term stability of the protocol.

2. Scope

This Security Policy applies to all currently active components of the Luxor ecosystem,
including:
• the staking and swap dApp available at stake.lxr-network.com,
• all associated smart contracts deployed on the Solana blockchain,

• and the project’s official GitHub repositories under github.com/Luxor-
Foundation.

It covers all activities related to the development, maintenance, deployment, and use
of the Luxor protocols.

This policy also applies to all individuals and organizations interacting with the project,
including developers, partners, community members, and external security
researchers.

The goal is to establish consistent standards for handling security-related matters and
to ensure that all reported vulnerabilities are addressed fairly, confidentially, and
efficiently.

3. Responsibilities

The security of the Luxor ecosystem is based on shared responsibility.
Developers, users, and external security researchers all play a crucial role in
maintaining the integrity and stability of the system.

a) Development Team
• is responsible for the secure development, implementation, and maintenance of
all smart contracts,
• performs regular internal code reviews and testing,
• responds promptly to reported vulnerabilities and takes appropriate mitigation
measures,
• ensures transparent communication regarding any security-related updates or
protocol changes.

b) Users and Community Members
• must handle their wallets and private keys responsibly,
• must not exploit vulnerabilities or system weaknesses for personal gain,
• are encouraged to report any potential security concerns or irregularities to the
team immediately.

c) External Security Researchers
• may contribute by identifying vulnerabilities and reporting them according to
responsible disclosure guidelines,
• must not publicly disclose or exploit any findings before a fix has been deployed,
• may, upon request, receive public acknowledgment (Disclosure
Acknowledgement) for valid reports.

Luxor Network is committed to evaluating all reports seriously, acting transparently,
and continuously improving the security and reliability of the protocol.

4. Responsible Disclosure
Luxor Network encourages the responsible and ethical reporting of security
vulnerabilities that may affect the functionality, integrity, or security of the Luxor staking
and swap dApps, associated smart contracts, or supporting infrastructure.
Security researchers and users should follow the guidelines below when reporting
issues:

a) Reporting Procedure
• Submit vulnerability reports privately to the Luxor security team at:
ulbrichtdominik565@gmail.com.
• Include sufficient information to reproduce the issue, such as:
o affected component (smart contract address, dApp URL, or API),
o clear description of the vulnerability,
o proof-of-concept or reproduction steps,
o potential impact or exploitation scenario, and
o any suggested mitigation measures, if available.
• If possible, provide transaction IDs, screenshots, or minimal test cases that help
reproduce the issue without exposing sensitive data.

b) Responsible Conduct

• Do not exploit the vulnerability for personal gain, to access funds, to disrupt
services, or to manipulate the system.
• Do not publicly disclose the vulnerability or share technical details with third
parties before the Luxor team has had a reasonable opportunity to investigate
and remediate.
• Allow the Luxor team reasonable time to respond and to implement fixes prior to
any public disclosure.

c) Acknowledgement, Triage & Communication
• Luxor will acknowledge receipt of reports as soon as possible (typically within 48
hours).
• The team will evaluate and triage submissions and provide an initial assessment
or timeline for remediation (typically within 7 days, depending on severity and
complexity).
• Luxor aims to keep reporters informed about status, remediation progress, and
resolution.
• Valid reports may be publicly acknowledged (Disclosure Acknowledgement)
upon request of the reporter and after the issue has been resolved.

d) Bug-Bounty (If Available)
• A formal Bug-Bounty / Vulnerability Reward Program may be established in the
future.
• If and when such a program is activated, Luxor will publish full program terms —
including scope, severity-based reward tiers, payout methods, and any KYC
requirements — on the official website and in the project repository.
• Until the program is formally activated, reports should be submitted via the
email above; Luxor may, at its discretion, provide recognition or discretionary
rewards for valid submissions.

e) Safe-Harbor & Legal Considerations
• Provided that researchers act in good faith and follow these responsible
disclosure guidelines, Luxor will not pursue legal action against them for actions
reasonably undertaken to test and report vulnerabilities.

• This Safe-Harbor does not cover activities that involve exfiltration of data, theft
of funds, exploitation for personal gain, or public disclosure that causes material
harm prior to remediation.

5. Security Measures

Luxor Network implements a multi-layered security framework to ensure the integrity,
transparency, and resilience of its entire ecosystem.
Both technical and organizational measures are applied to minimize risk and maintain
user trust over the long term.

a) Technical Measures
• Verified Smart Contracts:
All Luxor smart contracts are publicly accessible, verified on the Solana blockchain,
and viewable through platforms such as Solscan and SolanaFM.
• Fixed Token Supply:
Since no additional LXR tokens can be minted, the risk of hidden inflation or supply
manipulation is eliminated.
• LP-Locker Mechanism:
All liquidity pool (LP) tokens are permanently locked through a dedicated and verified
smart contract, preventing any unauthorized withdrawal of liquidity.
• Non-Custodial Architecture:
Users retain full control over their wallets and tokens at all times. Luxor never has direct
access to user assets.
• Secure Wallet Connections:
All interactions with the dApp occur exclusively over encrypted HTTPS connections and
through verified Solana wallet adapters.

• Controlled Permissions:
Administrative functions — such as adjusting payout intervals or initiating emergency
stops — are clearly defined, restricted to authorized wallets, and fully logged.
• Monitoring and Logging:
On-chain activities, including buybacks, reward distributions, and liquidity movements,
are continuously monitored to detect and respond to irregularities early.

b) Organizational Measures
• Internal Code Reviews:
Regular internal code reviews and testing are performed to identify and fix
vulnerabilities.
• Planned External Audit:
An independent third-party security audit is part of the Luxor roadmap to provide
additional transparency and external validation.
• Continuous Improvement:
Security is treated as an ongoing process. Lessons learned from audits, community
feedback, and responsible disclosures are actively integrated into future updates.
• Incident Response Procedures:
Internal protocols are in place to ensure rapid action in the event of critical security
incidents, minimizing potential damage or impact.

c) No Absolute Guarantee

Despite the implementation of comprehensive technical and organizational
safeguards, absolute security cannot be guaranteed.
Use of the Luxor dApp and interaction with its smart contracts are at the user’s own
risk.

6. Contact & Policy Updates

a) Contact for Security Reports

For all security-related inquiries, notifications, or vulnerability reports, please contact
the Luxor security team at:
📧ulbrichtdominik565@gmail.com

Please note:
• This email address should be used exclusively for security-related topics.
• Include as many relevant technical details as possible to help reproduce and
assess the issue.
• Do not send sensitive data such as private keys, passwords, or personal
information.

Luxor Network is committed to treating all reports confidentially and responding within
a reasonable timeframe.
b) Scope of Responsibility
This Security Policy applies only to systems, smart contracts, and dApps that are
officially operated or verified by Luxor Network.
External projects, partners, or third-party integrations are not covered by this policy.

c) Policy Updates

Luxor Network reserves the right to update or amend this Security Policy at any time.

Such updates may be necessary to:
• address new technical developments,
• comply with regulatory or legal requirements, or
• respond to security incidents and newly identified risks.

Significant changes will be announced through Luxor’s official communication
channels (e.g., X/Twitter, Telegram).

d) Last Updated

Last updated: October 2025