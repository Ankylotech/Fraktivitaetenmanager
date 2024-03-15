# Funktionsweise

### Basics

Die Fraktivitäten benötigen eine Dauer, einen Namen und eine Liste an Teilnehmern und "akzeptablen" Räumen. 
Für fixe Fraktivitaeten kann auch ein bestimmter Raum und Zeitpunkt angegeben werden, an dem diese stattfindet.
Optional kann bei Fraktivitaeten angegeben werden, welche Zeiträume für die Durchführung ausgeschlossen sind.
Zusätzlich kann angegeben werden, wie viel vor- bzw. Nachbereitungszeit für diese gebraucht wird. Standardmäßig 
gibt es für eine Fraktivität 15 Minuten Vorbereitungszeit und 0 Minuten Nachbereitungszeit. 

### Funktionalität

Die Verteilung errfolgt generell über recursive Backtracking. Die Fraktivitäten werden also nacheinander in einem Raum und zu einem Zeitpunkt 
verteilt und dann überprüft ob die übrigen Fraktivitäten sonst noch verteilt werden können. Ist dies nicht der Fall, 
wird eine andere Stelle für die Fraktivität ausprobiert. 

Die Fraktivitäten und Zeitpunkte werden dabei sortiert, so dass Lösungen bei denen die Fraktivitäten gut über den Tag verteilt sind zuerst gefunden werden.

