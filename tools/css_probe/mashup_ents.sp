#include <sourcemod>
#include <sdktools>

public Plugin myinfo = { name = "mashup_ents", author = "mashup", description = "entity list for round restart checks", version = "1" };

public void OnPluginStart()
{
    RegServerCmd("mashup_ents", Command_Ents,
        "mashup_ents <file>: every entity (networked or not) as 'ref class hammerid targetname' lines");
    // logic_auto's outputs, each as it fires (entity_io.md question 1).
    char outs[][] = {"OnMapSpawn", "OnNewGame", "OnLoadGame", "OnMapTransition", "OnBackgroundMap",
        "OnMultiNewMap", "OnMultiNewRound"};
    for (int i = 0; i < sizeof(outs); i++) {
        HookEntityOutput("logic_auto", outs[i], Output_Auto);
    }
}

public void Output_Auto(const char[] output, int caller, int activator, float delay)
{
    char map[64];
    GetCurrentMap(map, sizeof(map));
    int hammer = IsValidEntity(caller) && HasEntProp(caller, Prop_Data, "m_iHammerID") ? GetEntProp(caller, Prop_Data, "m_iHammerID") : -1;
    LogToFile("addons/sourcemod/logs/mashup_auto.log", "%s logic_auto %d %s t=%.3f", map, hammer, output, GetGameTime());
}

public Action Command_Ents(int args)
{
    char path[PLATFORM_MAX_PATH];
    GetCmdArg(1, path, sizeof(path));
    File f = OpenFile(path, "w");
    if (f == null) {
        PrintToServer("mashup_ents: cannot write %s", path);
        return Plugin_Handled;
    }
    int n = 0;
    int e = -1;
    char cls[64];
    char name[128];
    while ((e = FindEntityByClassname(e, "*")) != -1) {
        int ref = (e & (1 << 31)) ? e : EntIndexToEntRef(e);
        GetEntityClassname(e, cls, sizeof(cls));
        name[0] = '\0';
        if (HasEntProp(e, Prop_Data, "m_iName")) {
            GetEntPropString(e, Prop_Data, "m_iName", name, sizeof(name));
        }
        int hammer = HasEntProp(e, Prop_Data, "m_iHammerID") ? GetEntProp(e, Prop_Data, "m_iHammerID") : -1;
        f.WriteLine("%d %s %d %s", ref, cls, hammer, name);
        n++;
    }
    delete f;
    PrintToServer("mashup_ents: %d entities", n);
    return Plugin_Handled;
}
