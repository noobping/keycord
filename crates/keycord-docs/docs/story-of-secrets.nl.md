# Het verhaal van geheimen

Dit is een codegerichte rondgang door hoe Keycord geheime gegevens verplaatst vanaf het maken van een opslag tot het kopiëren van een wachtwoord.

## Verhaal 1: Een opslag wordt geboren

De opslagflow begint in de [beheer-UI](../../keycord-stores/src/ui/management/mod.rs) van Stores, met de regels voor mapselectie in het [opslagbeheerbeleid](../../keycord-stores/src/management.rs). Wanneer de gebruiker een map kiest, controleert Keycord of die leeg is.

Als de map niet leeg is, behandelt Keycord die als een bestaande opslag en opent het de editor voor opslagsleutels.

Als de map leeg is, opent Keycord via de [controller voor de ontvangerspagina](../../keycord-stores/src/ui/recipient_page/mod.rs) van Stores de versie van de pagina voor het maken van een nieuwe opslag. De modus voor maken zet meteen een autosave in de wachtrij, maar die opslag wordt pas echt zodra er minstens een ontvanger is.

De ontvangerspagina houdt een lijst van geselecteerde ontvangers in het geheugen. Voor het opslaan normaliseert het [ontvangerbeleid van Stores](../../keycord-stores/src/recipients.rs) die lijst naar standaardontvangers die in `.gpg-id` horen.

De opslagactie zit in de [opslagcontroller van de ontvangerspagina](../../keycord-stores/src/ui/recipient_page/save.rs). De bestandssysteemtransactie wordt uitgevoerd door [de geïntegreerde Stores-backend](../../keycord-stores/src/integrated.rs), terwijl de [compositie-adapter](../../../src/composition/backend/integrated/store.rs) in de root de crypto van Entries en de Git-effecten levert:

1. Keycord verzamelt de huidige ontvangers en de huidige eis voor privésleutels.
2. `save_store_recipients` zorgt dat de opslagmap bestaat.
3. Eerst wordt elk bestaand item ontsleuteld.
4. Daarna worden de bijgewerkte ontvangerbestanden geschreven.
5. Vervolgens wordt elk item opnieuw versleuteld onder het nieuwe beleid.
6. Als de opslag splinternieuw is, kan Keycord ook Git initialiseren.

Twee details zijn hier belangrijk.

Ten eerste zijn ontvangerbestanden transactioneel. De [padafhandeling van Stores](../../keycord-stores/src/paths.rs) schrijft het nieuwe `.gpg-id`-ontvangerbestand, voert de closure voor herencryptie uit en zet het oude bestand terug als de herencryptie mislukt.

Ten tweede worden ontvangers per pad geerfd. De [padafhandeling van Stores](../../keycord-stores/src/paths.rs) zoekt de ontvangers van een item door omhoog te lopen totdat het de dichtstbijzijnde `.gpg-id` vindt. Het "verhaal van een geheim" is dus eigenlijk "vind het dichtstbijzijnde ontvangerbestand en gebruik dan dat beleid".

## Verhaal 2: Een geheim wordt geschreven

Het dialoogvenster voor een nieuw item wordt gebouwd in de [UI voor nieuwe items](../../keycord-entries/src/ui/new_item.rs) van Entries. Het kiest een opslagroot en een label voor het pass-bestand zoals `team/service`.

Wanneer de editor opent in de [controller voor de wachtwoordpagina](../../keycord-entries/src/ui/page/mod.rs) van Entries, vult Keycord het nieuwe bestand eerst met het "sjabloon voor nieuwe wachtwoorden" uit Voorkeuren. De [samenstelling van pass-bestanden](../../keycord-entries/src/file/compose.rs) zet dat sjabloon om in initiële platte tekst waarbij:

- de eerste regel het wachtwoordvak is
- latere regels gestructureerde velden zijn zoals `username:` of `url:`

Terwijl de gebruiker bewerkt, bouwen de [editor-UI](../../keycord-entries/src/ui/page/editor.rs) van Entries en de [samenstelling van pass-bestanden](../../keycord-entries/src/file/compose.rs) de tekst van het pass-bestand steeds opnieuw in het geheugen op. Keycord versleutelt niet veld voor veld. Het stelt altijd eerst een volledig pass-bestand in platte tekst samen en versleutelt daarna het geheel.

Bij opslaan roept de [controller voor de wachtwoordpagina](../../keycord-entries/src/ui/page/mod.rs) de [backenddispatcher](../../../src/composition/backend/mod.rs) in de root aan, die de actieve backend kiest. Voor de geïntegreerde backend levert de [Entries-compositie-adapter](../../../src/composition/backend/integrated/entries.rs) in de root de poorten voor Keys, Stores en Git aan de [geïntegreerde invoermotor](../../keycord-entries/src/integrated.rs) van Entries.

Dat opslagpad doet vier belangrijke dingen:

1. Het bepaalt het definitieve bestandspad voor het label.
2. Het laadt de cryptocontext uit de dichtstbijzijnde ontvangerbestanden.
3. Het versleutelt de platte tekst volgens het opslagbeleid.
4. Het schrijft de ciphertext naar schijf.

De bestandsextensie hoort bij de standaard pass-indeling. De [padafhandeling](../../keycord-stores/src/paths.rs) en het [beleid voor invoerbestanden](../../keycord-stores/src/entry_files.rs) van Stores gebruiken `.gpg` voor wachtwoordinvoeren.

## Verhaal 3: Met wachtwoord beveiligde sleutel

Dit is het normale pad voor een beheerde sleutel.

De UI voor het genereren van de sleutel zit in de [controller voor privésleutelbeheer](../../keycord-keys/src/ui/key_management/private.rs) van Keys. De echte sleutelaanmaak en import gebeuren in de [opslag voor beheerde sleutels](../../keycord-keys/src/store/storage.rs):

1. `generate_ripasso_private_key` maakt een Sequoia-certificaat met een verplichte passphrase.
2. Het serialiseert het geheime sleutelmateriaal.
3. Het importeert dat materiaal meteen terug in de opslag voor beheerde sleutels van Keycord.

Import gebruikt dezelfde opslagmodule. De belangrijke regel wordt afgedwongen in de [opslag voor beheerde sleutels](../../keycord-keys/src/store/storage.rs): Keycord weigert een onbeveiligde softwarematige privésleutel te bewaren. Geimporteerde softwaresleutels moeten al met een wachtwoord zijn beveiligd.

Ontgrendelen is sessiegebaseerd. De [ontgrendel-UI](../../keycord-keys/src/ui/unlock.rs) van Keys verzamelt de passphrase, waarna de [ontgrendellogica voor beheerde sleutels](../../keycord-keys/src/store/unlock.rs) de opgeslagen sleutel ontsleutelt en het ontgrendelde certificaat cachet in de [sessiecache van Keys](../../keycord-keys/src/cache.rs).

Wanneer een item wordt gelezen, krijgt de [geïntegreerde invoermotor](../../keycord-entries/src/integrated.rs) van Entries zijn kandidatenlijst via het [geïntegreerde ontvangerbeleid](../../keycord-stores/src/integrated_recipients.rs) van Stores:

- ontvangers voor het item
- de geselecteerde "eigen" vingerafdruk, als die is geconfigureerd
- elke geïmporteerde beheerde sleutel

Als de benodigde sleutel nog vergrendeld is, faalt het lezen met een locked-key-fout. De copy- en open-flow vangen die fout af en leiden terug naar de ontgrendeldialoog via de [klembordcontroller](../../keycord-entries/src/clipboard.rs) van Entries of de [ontgrendel-UI](../../keycord-keys/src/ui/unlock.rs) van Keys.

Voor versleuteling bouwt de [geïntegreerde cryptocontext](../../keycord-entries/src/integrated.rs) van Entries een normale lijst met OpenPGP-ontvangers en versleutelt het het hele pass-bestand in een keer.

## Verhaal 4: Alle sleutels verplichten (experimenteel)

Deze experimentele optie combineert de [controller voor de ontvangstsleutellijst](../../keycord-keys/src/ui/key_management/recipient_list.rs) van Keys, die de beschikbare privésleutels en hun selectieacties toont, met de [controller voor de ontvangerspagina](../../keycord-stores/src/ui/recipient_page/list.rs) van Stores, die de schakelaar "alle sleutels vereisen" toont en het ontvangerbeleid van de opslag toepast.

Het opslaan van die optie maakt geen nieuw bestand. Het voegt metadata toe aan `.gpg-id`. Het [geïntegreerde ontvangerbeleid](../../keycord-stores/src/integrated_recipients.rs) van Stores schrijft:

```text
# keycord-private-key-requirement=all
```

Die ene comment verandert het hele lees- en schrijfpak.

Bij schrijven schakelt de [geïntegreerde cryptocontext](../../keycord-entries/src/integrated.rs) van Entries over van "elke geselecteerde sleutel mag dit openen" naar experimentele gelaagde versleuteling:

1. Versleutel de platte tekst voor de binnenste vereiste ontvanger.
2. Wikkel die ciphertext in een laag `keycord-require-all-private-keys-v1`.
3. Versleutel die ingepakte waarde voor de volgende ontvanger.
4. Herhaal dit totdat elke vereiste sleutel een laag heeft toegevoegd.

Bij lezen draait dezelfde module dat proces voor elke ontvanger in omgekeerde volgorde terug. Als ook maar een vereiste sleutel ontbreekt, incompatibel is of nog vergrendeld is, gaat het geheim niet open.

## Verhaal 5: Experimentele met FIDO2 beveiligde privésleutel

De flow voor met FIDO2 beveiligde privésleutels begint in de [controller voor privésleutelbeheer](../../keycord-keys/src/ui/key_management/private.rs) van Keys. Apparaattransport, binding en enveloplogica zitten in de aparte [FIDO-crate](../../keycord-fido/src), Keys past die service aan via zijn [FIDO-integratie](../../keycord-keys/src/fido2) en de beveiligde privésleutelbytes worden opgeslagen via de [opslag voor beheerde sleutels](../../keycord-keys/src/store/storage.rs).

Wanneer de gebruiker een experimentele met FIDO2 beveiligde privésleutel genereert:

1. registreert Keycord een `hmac-secret`-credential tegen de Keycord RP ID
2. maakt het een `FidoBindingDescriptor` met de sleutelvingerafdruk, weergavelabel en credential-id
3. slaat het die descriptor in het privésleutelmanifest op naast het beveiligde sleutelmateriaal
4. versleutelt het de beveiligingslaag van de privésleutel met het FIDO2 direct required-layer-formaat

Die descriptor is metadata voor privésleutels. Het is geen opslagontvanger, hij wordt niet naar `.gpg-id` geschreven en Keycord schrijft geen FIDO2-sidecarbestand meer.

Ontgrendelen blijft sessiegebaseerd. De [ontgrendel-UI](../../keycord-keys/src/ui/unlock.rs) van Keys kan om een FIDO2-PIN vragen, waarna de [ontgrendellogica voor beheerde sleutels](../../keycord-keys/src/store/unlock.rs) de FIDO-service vraagt het apparaat te valideren. De [FIDO-cache](../../keycord-fido/src/cache.rs) bewaart de PIN voor de sessie, terwijl de [Keys-cache](../../keycord-keys/src/cache.rs) het ontgrendelde OpenPGP-certificaat bewaart. Eenmaal ontgrendeld doet die beheerde sleutel mee in de normale ontvangerflow hierboven.

## Verhaal 6: Een geheim wordt geopend

Het openen van een wachtwoordinvoer begint in de [controller voor de wachtwoordpagina](../../keycord-entries/src/ui/page/mod.rs) van Entries. De pagina toont een laadstatus en roept daarna `read_password_entry_with_progress` aan.

Het geïntegreerde leespad in de [geïntegreerde invoermotor](../../keycord-entries/src/integrated.rs) van Entries, bereikt via de [Entries-compositie-adapter](../../../src/composition/backend/integrated/entries.rs) in de root, splitst op basis van de eis voor privésleutels:

- `AnyManagedKey`: probeer kandidaten totdat er een ontsleutelt
- `AllManagedKeys`: vereis elke geselecteerde sleutel in volgorde

De cryptocontext komt uit de [geïntegreerde Entries-motor](../../keycord-entries/src/integrated.rs). De kandidatenlijst en ontvangermetadata komen uit het [geïntegreerde ontvangerbeleid](../../keycord-stores/src/integrated_recipients.rs) van Stores.

Als het item opent, gaat het pass-bestand in platte tekst terug naar de gestructureerde editor.

Als de sleutel vergrendeld is, geeft Keycord een getypeerde fout door vanuit de [fouttypen van Entries](../../keycord-entries/src/error.rs), zodat de UI de ontbrekende ontgrendelstap kan vragen in plaats van alleen te falen.

## Verhaal 7: Het wachtwoord kopiëren

De kopieerknop op elke wachtwoordrij wordt gekoppeld in de [UI voor lijstrijen](../../keycord-entries/src/ui/list/row.rs) van Entries. Die roept de [klembordcontroller](../../keycord-entries/src/clipboard.rs) van Entries aan.

Vanaf daar is het verhaal kort:

1. Als de geïntegreerde backend actief is, leest Keycord alleen de eerste regel van het item via `read_password_line`.
2. Als dat lezen mislukt omdat de sleutel vergrendeld is, zoekt Keycord de voorkeursleutel op en toont het de ontgrendeldialoog.
3. Als het lezen lukt, schrijft Keycord de eerste regel naar het klembord van het systeem en toont het knopfeedback.

Het belangrijke detail is dat kopiëren nog steeds een ontsleuteloperatie is. Het wachtwoord wordt nergens anders in de app als kant-en-klare platte tekst voor kopiëren gecachet. Keycord gaat opnieuw door hetzelfde leespad, neemt de eerste regel en geeft die tekst aan het klembord.

Als de Host-backend actief is, neemt de [klembordcontroller](../../keycord-entries/src/clipboard.rs) een andere poort die wordt geleverd door de [Entries-compositie-adapter](../../../src/composition/entries_ui.rs) in de root, die vervolgens `pass -c` aanroept. De rest van deze handleiding volgt het geïntegreerde pad, omdat daar het beheer van opslagsleutels, experimentele gelaagde versleuteling en experimenteel FIDO2-gedrag leeft.
