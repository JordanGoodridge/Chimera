#include <sourcemod>
#include <sdktools>

public Plugin myinfo = { name = "mashup_ents", author = "mashup", description = "entity list for round restart checks", version = "1" };

public void OnPluginStart()
{
    RegServerCmd("mashup_ents", Command_Ents,
        "mashup_ents <file>: every entity (networked or not) as 'ref class hammerid targetname' lines");
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
