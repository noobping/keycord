# Keycord-documentatie

Keycord is een grafische app voor standaard [`pass`](https://www.passwordstore.org/)-opslagen. Het behoudt dezelfde mapindeling op schijf, werkt met compatibele pass-hulpmiddelen en gebruikt een adaptieve GTK-interface voor toetsenbord, muisaanwijzer en aanraking op Linux-desktops en mobiele Linux-apparaten.

## Handleidingen

- [Aan de slag](getting-started.md): installatie, opslagen, eerste items en eerste zoekopdrachten
- [Zoekgids](search.md): gewoon zoeken, `reg` en `find`
- [Werkstromen](workflows.md): bewerken, OTP, passkeys, hulpmiddelen, export, sneltoetsen en onderhoud
- [Machtigingen en backends](permissions-and-backends.md): Integrated vs Host, Flatpak-machtigingen, Git en sleutelsynchronisatie
- [Het verhaal van geheimen](story-of-secrets.md): codegerichte rondgang door het maken van opslagen, versleuteling van items, ontgrendelpaden en kopiëren naar het klembord
- [Teams, werkgroepen en organisaties](teams-and-organizations.md): gedeelde opslagen, ontvangers, onboarding, offboarding en bootstrap-patronen
- [Gebruiksscenario's](use-cases.md): veelvoorkomende opstellingen, van persoonlijk gebruik tot gedeelde opslagen en beheerwerk

## Standaardindeling

Keycord leest en schrijft gewone `pass`-opslagen:

- een opslagmap zoals `~/.password-store`
- één geheim per bestand
- de eerste regel als wachtwoord
- latere `key: value`-regels als gestructureerde velden
- een gereserveerd `passkey:`-veld voor passkeygegevens wanneer passkeyondersteuning is ingeschakeld
- `.gpg-id` voor opslagontvangers

## Keycord-functies

- open een of meer wachtwoordopslagen en zoek op naam, opslag, veld, reguliere expressie of gestructureerde `find`-zoekopdracht
- bewerk items met formuliervelden of ruwe pass-bestandstekst, genereer wachtwoorden en kopieer of toon wachtwoorden, gebruikersnamen en eenmalige codes als QR-code
- importeer passkeys in gewone versleutelde `pass`-items en open lokale verzoeken voor de uitwisseling van inloggegevens
- voeg bestaande opslagen toe, maak nieuwe opslagen, importeer wachtwoorden op ondersteunde systemen of herstel een opslag uit Git met de Host-backend
- beheer opslagontvangers, mapspecifieke `.gpg-id`-bestanden en met een wachtwoord beveiligde privésleutels, inclusief import uit bestanden of het klembord en optionele synchronisatie met GPG op de host
- vind zwakke wachtwoorden, bekijk terugkerende veldwaarden, filter op opslag en exporteer wachtwoordopslagen naar CSV
- synchroniseer Git-opslagen, beheer remotes, onderteken wijzigingen en bekijk de geschiedenis met details over commitverificatie
- gebruik de adaptieve GTK-interface met toetsenbord, muisaanwijzer of aanraking op Linux-desktops en mobiele Linux-apparaten

## Backendmatrix

| Mogelijkheid | Integrated | Host | Opmerkingen |
| --- | --- | --- | --- |
| Standaard-`pass`-opslagen bekijken en bewerken | Ja | Ja | Beide gebruiken de standaard opslagindeling. |
| Een aangepaste `pass`-opdracht gebruiken | Nee | Ja | Alleen Linux; stel de opdracht in bij Voorkeuren. |
| Zoeken, OTP, veldwaardebrowser, hulpmiddel voor zwakke wachtwoorden | Ja | Ja | Zoekgedrag is hetzelfde. |
| Opslagontvangers en door de app beheerde privésleutels beheren | Ja | Ja | Host-GPG-inspectie hangt af van hosttoegang. |
| Een opslag herstellen vanuit een Git-URL in de UI | Nee | Ja | Alleen Linux; hosttoegang vereist. |
| `pass import`-integratie | Nee | Ja | Alleen Linux; vereist de extensie `pass import`. |
| Git op afstand ophalen, mergen en pushen | Ja | Ja | Alleen Linux; vereist hosttoegang en een opslag met Git-backend. |
| Keycord-privésleutels synchroniseren met host-GPG | Ja | Ja | Alleen Linux en hosttoegang vereist. |

## Beperkingen

- Flatpak zonder hosttoegang:
  - Functies die alleen in Host beschikbaar zijn, zoals herstellen vanuit Git en `pass import`, blijven uitgeschakeld.
  - Als Host is geselecteerd zonder hosttoegang, valt Keycord terug op de Integrated-backend.
- Niet-Linux-builds:
  - Functies die alleen in Host beschikbaar zijn, zoals een aangepaste `pass`, herstellen vanuit Git en `pass import`, blijven verborgen.
- Experimentele gelaagde versleuteling:
  - dit is experimenteel en Keycord-specifiek
  - andere `pass`-apps kunnen die items niet lezen
- Passkeys:
  - passkeyondersteuning is standaard ingeschakeld en kan tijdens het bouwen worden weggelaten door de Cargo-feature `passkey` uit te schakelen
  - bij het openen van een CXP-verzoek controleert Keycord de structuur van een lokaal exportverzoek; dit is geen live passkeyprovider voor webbrowsers

## Begin

1. Lees [Aan de slag](getting-started.md).
2. Houd [Zoekgids](search.md) open terwijl je zoekopdrachten opbouwt.
3. Gebruik [Machtigingen en backends](permissions-and-backends.md) als een functie is uitgeschakeld.
