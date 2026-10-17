#include <sourcemod>
#include <sdktools>
#include <sdkhooks>

public Plugin myinfo = { name = "mashup_ents", author = "mashup", description = "entity list for round restart checks", version = "1" };

public void OnPluginStart()
{
    RegServerCmd("mashup_htrack", Command_HTrack, "mashup_htrack <entity|-1> [file]: log that entity's origin every server frame to the file (game time x y z); -1 stops");
    RegServerCmd("mashup_hosthook", Command_HostHook, "mashup_hosthook: hook every hostage's TraceAttack and OnTakeDamage (logged)");
    // Hostage and round events with the hostage's origin (objectives.md Q11).
    char evs[][] = {"hostage_follows", "hostage_stops_following", "hostage_rescued", "hostage_rescued_all",
        "hostage_killed", "hostage_hurt", "round_end", "round_start"};
    for (int i = 0; i < sizeof(evs); i++) {
        HookEvent(evs[i], Event_Host);
    }
    RegServerCmd("mashup_norescue", Command_NoRescue,
        "mashup_norescue <0|1>: remove every func_hostage_rescue as it spawns (then changelevel: a map with none)");
    RegServerCmd("mashup_hostages", Command_Hostages, "mashup_hostages: each hostage_entity's index, health, origin, model and skin");
    RegServerCmd("mashup_ents", Command_Ents,
        "mashup_ents <file>: every entity (networked or not) as 'ref class hammerid targetname' lines");
    // logic_auto's outputs, each as it fires (entity_io.md question 1).
    char outs[][] = {"OnMapSpawn", "OnNewGame", "OnLoadGame", "OnMapTransition", "OnBackgroundMap",
        "OnMultiNewMap", "OnMultiNewRound"};
    for (int i = 0; i < sizeof(outs); i++) {
        HookEntityOutput("logic_auto", outs[i], Output_Auto);
    }
}

public void Event_Host(Event event, const char[] name, bool dontBroadcast)
{
    int h = event.GetInt("hostage");
    float o[3];
    if (h > 0 && IsValidEntity(h)) {
        GetEntPropVector(h, Prop_Send, "m_vecOrigin", o);
    }
    LogToFile("addons/sourcemod/logs/mashup_host.log", "event %s t=%.3f k=%d user=%d hostage=%d at %.1f %.1f %.1f winner=%d reason=%d",
        name, GetGameTime(), GetGameTickCount(), event.GetInt("userid"), h, o[0], o[1], o[2],
        event.GetInt("winner"), event.GetInt("reason"));
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

public Action Command_Hostages(int args)
{
    int e = -1;
    while ((e = FindEntityByClassname(e, "hostage_entity")) != -1) {
        float o[3];
        GetEntPropVector(e, Prop_Send, "m_vecOrigin", o);
        char model[PLATFORM_MAX_PATH];
        GetEntPropString(e, Prop_Data, "m_ModelName", model, sizeof(model));
        PrintToServer("hostage %d health %d at %.1f %.1f %.1f model %s skin %d", e, GetEntProp(e, Prop_Data, "m_iHealth"), o[0], o[1], o[2],
            model, GetEntProp(e, Prop_Send, "m_nSkin"));
    }
    return Plugin_Handled;
}

// mashup_norescue: remove every func_hostage_rescue as it is created, so a
// map (re)loads with none (objectives.md Q11).
bool g_noRescue;

public Action Command_NoRescue(int args)
{
    g_noRescue = GetCmdArgInt(1) != 0;
    PrintToServer("mashup_norescue %d", g_noRescue);
    return Plugin_Handled;
}

public void OnEntityCreated(int entity, const char[] classname)
{
    if (g_noRescue && StrEqual(classname, "func_hostage_rescue")) {
        SDKHook(entity, SDKHook_SpawnPost, Spawned_Rescue);
    }
}

public void Spawned_Rescue(int entity)
{
    RemoveEntity(entity);
    LogToFile("addons/sourcemod/logs/mashup_host.log", "removed func_hostage_rescue %d t=%.3f", entity, GetGameTime());
}

int g_htrack = -1;
File g_hfile = null;

public Action Command_HTrack(int args)
{
    if (g_hfile != null) {
        delete g_hfile;
    }
    g_htrack = GetCmdArgInt(1);
    if (g_htrack > 0) {
        char path[PLATFORM_MAX_PATH];
        GetCmdArg(2, path, sizeof(path));
        g_hfile = OpenFile(path, "w");
    }
    return Plugin_Handled;
}

public void OnGameFrame()
{
    if (g_hfile == null || g_htrack <= 0 || !IsValidEntity(g_htrack)) {
        return;
    }
    float o[3];
    GetEntPropVector(g_htrack, Prop_Send, "m_vecOrigin", o);
    g_hfile.WriteLine("%.4f %.3f %.3f %.3f", GetGameTime(), o[0], o[1], o[2]);
    g_hfile.Flush();
}

public Action Command_HostHook(int args)
{
    int e = -1;
    int n = 0;
    while ((e = FindEntityByClassname(e, "hostage_entity")) != -1) {
        SDKHook(e, SDKHook_TraceAttack, Hook_Trace);
        SDKHook(e, SDKHook_OnTakeDamage, Hook_Damage);
        n++;
    }
    PrintToServer("hooked %d hostages", n);
    return Plugin_Handled;
}

public Action Hook_Trace(int victim, int &attacker, int &inflictor, float &damage, int &damagetype, int &ammotype, int hitbox, int hitgroup)
{
    LogToFile("addons/sourcemod/logs/mashup_host.log", "trace victim %d damage %.3f hitbox %d hitgroup %d t=%.3f", victim, damage, hitbox, hitgroup, GetGameTime());
    return Plugin_Continue;
}

public Action Hook_Damage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype)
{
    LogToFile("addons/sourcemod/logs/mashup_host.log", "damage victim %d damage %.3f type %d t=%.3f", victim, damage, damagetype, GetGameTime());
    return Plugin_Continue;
}
