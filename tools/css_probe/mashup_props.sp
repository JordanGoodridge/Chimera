// mashup_props: prop and breakable damage probes (specs/source/prop_damage.md,
// breakables.md, doors_buttons.md open questions; tools/css_probe/README.md
// "Prop damage").
//
// Every line goes to addons/sourcemod/logs/mashup_props.log as "t=<game time>
// <what> ...":
//   dmg   <victim> <class> hp=<before> dmg=<damage> type=<bits> att=<attacker> inf=<inflictor class> wpn=<weapon class>
//   post  <victim> <class> hp=<after> dmg=<damage taken> type=<bits>
//   out   <caller> <class> <output> act=<activator>
//   snd   <entity> <class> ch=<channel> vol=<volume> lvl=<level> <sample>
//   fire  <client> ...  (mashup_fire's attack ticks)
// Server commands: see OnPluginStart.

#include <sourcemod>
#include <sdktools>
#include <sdkhooks>
#include <cstrike>

public Plugin myinfo = { name = "mashup_props", author = "mashup", description = "prop damage probes", version = "1" };

#define LOG "addons/sourcemod/logs/mashup_props.log"

// mashup_fire state.
int g_fireClient;
float g_firePos[3];
float g_fireAng[3];
int g_fireButtons;
int g_fireStart;
int g_fireTicks;
int g_fireTick;
bool g_soundLog;

public void OnPluginStart()
{
    RegServerCmd("mashup_dmghook", Command_DmgHook,
        "mashup_dmghook <classname|index>: log OnTakeDamage before and after on those entities");
    RegServerCmd("mashup_spawn", Command_Spawn,
        "mashup_spawn <class> <model> <x> <y> <z> <pitch> <yaw> [key value]...: create, dispatch and print the index");
    RegServerCmd("mashup_find", Command_Find,
        "mashup_find <classname> [model substring]: index, name, model, origin, health, mins/maxs of each");
    RegServerCmd("mashup_einfo", Command_EInfo,
        "mashup_einfo <index>: class, health, origin, velocity, effects, solid, movetype, flags, physics mode");
    RegServerCmd("mashup_fire", Command_Fire,
        "mashup_fire <userid> <x> <y> <z> <pitch> <yaw> <weapon> <buttons 1=attack 2=attack2> <start tick> <ticks>: hold that bot there with the weapon and press the buttons");
    RegServerCmd("mashup_hp", Command_Hp, "mashup_hp <index> <health> [armor] [helmet]: set an entity's health (players: respawn if dead, and armour)");
    RegServerCmd("mashup_kill", Command_Kill, "mashup_kill <index>: remove an entity");
    RegServerCmd("mashup_input", Command_Input, "mashup_input <index> <input> [parameter] [activator]: fire an input on an entity");
    RegServerCmd("mashup_watch", Command_Watch, "mashup_watch <index> <frames>: log the entity's health, effects, solid flags and render mode every frame (and when it is gone)");
    RegServerCmd("mashup_panes", Command_Panes, "mashup_panes <index>: a func_breakable_surf's size in panes, broken flag and its pane grid (1 = still there)");
    RegServerCmd("mashup_getprop", Command_GetProp, "mashup_getprop <index|-1> <datamap field> [f]: the field as an int (f: a float); -1: a game rules netprop");
    RegServerCmd("mashup_sndlog", Command_SndLog, "mashup_sndlog <0|1>: log every emitted sound");
    RegServerCmd("mashup_tp", Command_Tp, "mashup_tp <index> <x> <y> <z> [vx vy vz]: teleport an entity (with velocity)");
    RegServerCmd("mashup_hurt", Command_Hurt,
        "mashup_hurt <victim> <attacker> <damage> <type bits> [x y z]: SDKHooks_TakeDamage (attacker -1 = world)");
    char outs[][] = {"OnBreak", "OnHealthChanged", "OnTakeDamage", "OnAwakened", "OnMotionEnabled", "OnIgnite",
        "OnPhysGunPickup", "OnPlayerUse", "OnOpen", "OnClose", "OnFullyOpen", "OnFullyClosed"};
    char classes[][] = {"prop_physics", "prop_physics_multiplayer", "prop_physics_override", "prop_dynamic",
        "prop_dynamic_override", "func_breakable", "func_breakable_surf", "func_physbox", "func_physbox_multiplayer",
        "prop_door_rotating"};
    for (int c = 0; c < sizeof(classes); c++) {
        for (int i = 0; i < sizeof(outs); i++) {
            HookEntityOutput(classes[c], outs[i], Output_Any);
        }
    }
    AddNormalSoundHook(SoundHook);
    HookEvent("player_hurt", Event_Hurt);
}

// "hurt <victim> att=<attacker> hp=<health left> armor=<armour left>
// dh=<dmg_health> da=<dmg_armor> hg=<hitgroup> wpn=<weapon>" (the knife
// against armour, prop_damage.md / weapons.md).
public void Event_Hurt(Event event, const char[] name, bool dontBroadcast)
{
    char wpn[64];
    event.GetString("weapon", wpn, sizeof(wpn));
    Log("hurt %d att=%d hp=%d armor=%d dh=%d da=%d hg=%d wpn=%s", GetClientOfUserId(event.GetInt("userid")),
        GetClientOfUserId(event.GetInt("attacker")), event.GetInt("health"), event.GetInt("armor"),
        event.GetInt("dmg_health"), event.GetInt("dmg_armor"), event.GetInt("hitgroup"), wpn);
}

void Log(const char[] fmt, any ...)
{
    char buf[512];
    VFormat(buf, sizeof(buf), fmt, 2);
    LogToFile(LOG, "t=%.3f k=%d %s", GetGameTime(), GetGameTickCount(), buf);
}

public void Output_Any(const char[] output, int caller, int activator, float delay)
{
    char cls[64];
    GetEntityClassname(caller, cls, sizeof(cls));
    int hp = HasEntProp(caller, Prop_Data, "m_iHealth") ? GetEntProp(caller, Prop_Data, "m_iHealth") : -1;
    Log("out %d %s %s act=%d hp=%d", caller, cls, output, activator, hp);
}

public Action SoundHook(int clients[MAXPLAYERS], int &numClients, char sample[PLATFORM_MAX_PATH], int &entity,
    int &channel, float &volume, int &level, int &pitch, int &flags, char soundEntry[PLATFORM_MAX_PATH], int &seed)
{
    if (!g_soundLog) {
        return Plugin_Continue;
    }
    char cls[64] = "?";
    if (entity > 0 && IsValidEntity(entity)) {
        GetEntityClassname(entity, cls, sizeof(cls));
    }
    Log("snd %d %s ch=%d vol=%.2f lvl=%d pitch=%d entry=%s %s", entity, cls, channel, volume, level, pitch, soundEntry, sample);
    return Plugin_Continue;
}

public Action Command_SndLog(int args)
{
    g_soundLog = GetCmdArgInt(1) != 0;
    return Plugin_Handled;
}

void HookOne(int e)
{
    SDKHook(e, SDKHook_OnTakeDamage, Hook_Damage);
    SDKHook(e, SDKHook_OnTakeDamagePost, Hook_DamagePost);
}

public Action Command_DmgHook(int args)
{
    char arg[64];
    GetCmdArg(1, arg, sizeof(arg));
    int n = 0;
    if (arg[0] >= '0' && arg[0] <= '9') {
        HookOne(StringToInt(arg));
        n = 1;
    } else {
        int e = -1;
        while ((e = FindEntityByClassname(e, arg)) != -1) {
            HookOne(e);
            n++;
        }
    }
    PrintToServer("mashup_dmghook: %d hooked", n);
    return Plugin_Handled;
}

int Health(int e)
{
    return HasEntProp(e, Prop_Data, "m_iHealth") ? GetEntProp(e, Prop_Data, "m_iHealth") : -1;
}

public Action Hook_Damage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype, int &weapon,
    float damageForce[3], float damagePosition[3], int damagecustom)
{
    char cls[64];
    char icls[64] = "-";
    char wcls[64] = "-";
    GetEntityClassname(victim, cls, sizeof(cls));
    if (inflictor > 0 && IsValidEntity(inflictor)) {
        GetEntityClassname(inflictor, icls, sizeof(icls));
    }
    if (weapon > 0 && IsValidEntity(weapon)) {
        GetEntityClassname(weapon, wcls, sizeof(wcls));
    }
    Log("dmg %d %s hp=%d dmg=%.3f type=%d att=%d inf=%d:%s wpn=%s force=%.1f,%.1f,%.1f pos=%.1f,%.1f,%.1f", victim, cls,
        Health(victim), damage, damagetype, attacker, inflictor, icls, wcls, damageForce[0], damageForce[1],
        damageForce[2], damagePosition[0], damagePosition[1], damagePosition[2]);
    return Plugin_Continue;
}

public void Hook_DamagePost(int victim, int attacker, int inflictor, float damage, int damagetype)
{
    char cls[64];
    GetEntityClassname(victim, cls, sizeof(cls));
    Log("post %d %s hp=%d dmg=%.3f type=%d", victim, cls, Health(victim), damage, damagetype);
}

public Action Command_Spawn(int args)
{
    char cls[64];
    char model[PLATFORM_MAX_PATH];
    GetCmdArg(1, cls, sizeof(cls));
    GetCmdArg(2, model, sizeof(model));
    float pos[3];
    float ang[3];
    pos[0] = GetCmdArgFloat(3);
    pos[1] = GetCmdArgFloat(4);
    pos[2] = GetCmdArgFloat(5);
    ang[0] = GetCmdArgFloat(6);
    ang[1] = GetCmdArgFloat(7);
    int e = CreateEntityByName(cls);
    if (e == -1) {
        PrintToServer("mashup_spawn: cannot create %s", cls);
        return Plugin_Handled;
    }
    // "-": no model (point entities).
    if (!StrEqual(model, "-")) {
        if (!IsModelPrecached(model)) {
            PrecacheModel(model);
        }
        DispatchKeyValue(e, "model", model);
    }
    char key[64];
    char val[128];
    for (int i = 8; i + 1 <= args; i += 2) {
        GetCmdArg(i, key, sizeof(key));
        GetCmdArg(i + 1, val, sizeof(val));
        DispatchKeyValue(e, key, val);
    }
    DispatchKeyValueVector(e, "origin", pos);
    DispatchKeyValueVector(e, "angles", ang);
    if (!DispatchSpawn(e)) {
        PrintToServer("mashup_spawn: spawn failed");
        return Plugin_Handled;
    }
    ActivateEntity(e);
    TeleportEntity(e, pos, ang, NULL_VECTOR);
    HookOne(e);
    PrintToServer("mashup_spawn: %d hp=%d", e, Health(e));
    return Plugin_Handled;
}

void Describe(int e)
{
    char cls[64];
    char name[64] = "";
    char model[PLATFORM_MAX_PATH] = "";
    GetEntityClassname(e, cls, sizeof(cls));
    if (HasEntProp(e, Prop_Data, "m_iName")) {
        GetEntPropString(e, Prop_Data, "m_iName", name, sizeof(name));
    }
    if (HasEntProp(e, Prop_Data, "m_ModelName")) {
        GetEntPropString(e, Prop_Data, "m_ModelName", model, sizeof(model));
    }
    float o[3];
    float v[3];
    float mins[3];
    float maxs[3];
    GetEntPropVector(e, Prop_Data, "m_vecAbsOrigin", o);
    GetEntPropVector(e, Prop_Data, "m_vecAbsVelocity", v);
    if (HasEntProp(e, Prop_Send, "m_vecMins")) {
        GetEntPropVector(e, Prop_Send, "m_vecMins", mins);
        GetEntPropVector(e, Prop_Send, "m_vecMaxs", maxs);
    }
    int effects = HasEntProp(e, Prop_Send, "m_fEffects") ? GetEntProp(e, Prop_Send, "m_fEffects") : -1;
    int solid = HasEntProp(e, Prop_Send, "m_nSolidType") ? GetEntProp(e, Prop_Send, "m_nSolidType") : -1;
    int sflags = HasEntProp(e, Prop_Send, "m_usSolidFlags") ? GetEntProp(e, Prop_Send, "m_usSolidFlags") : -1;
    int cgroup = HasEntProp(e, Prop_Send, "m_CollisionGroup") ? GetEntProp(e, Prop_Send, "m_CollisionGroup") : -1;
    int mode = HasEntProp(e, Prop_Send, "m_iPhysicsMode") ? GetEntProp(e, Prop_Send, "m_iPhysicsMode") : -1;
    int takedmg = HasEntProp(e, Prop_Data, "m_takedamage") ? GetEntProp(e, Prop_Data, "m_takedamage") : -1;
    int maxhp = HasEntProp(e, Prop_Data, "m_iMaxHealth") ? GetEntProp(e, Prop_Data, "m_iMaxHealth") : -1;
    int flags = HasEntProp(e, Prop_Data, "m_fFlags") ? GetEntProp(e, Prop_Data, "m_fFlags") : -1;
    int skin = HasEntProp(e, Prop_Send, "m_nSkin") ? GetEntProp(e, Prop_Send, "m_nSkin") : -1;
    int body = HasEntProp(e, Prop_Send, "m_nBody") ? GetEntProp(e, Prop_Send, "m_nBody") : -1;
    float mass = HasEntProp(e, Prop_Data, "m_massScale") ? GetEntPropFloat(e, Prop_Data, "m_massScale") : -1.0;
    PrintToServer("%d %s '%s' %s at %.2f %.2f %.2f vel %.2f %.2f %.2f hp=%d/%d takedmg=%d box %.4f %.4f %.4f / %.4f %.4f %.4f effects=%d solid=%d sflags=%d cgroup=%d move=%d flags=%d mode=%d skin=%d body=%d massscale=%.2f",
        e, cls, name, model, o[0], o[1], o[2], v[0], v[1], v[2], Health(e), maxhp, takedmg, mins[0], mins[1], mins[2],
        maxs[0], maxs[1], maxs[2], effects, solid, sflags, cgroup, view_as<int>(GetEntityMoveType(e)), flags, mode, skin,
        body, mass);
}

public Action Command_EInfo(int args)
{
    int e = GetCmdArgInt(1);
    if (!IsValidEntity(e)) {
        PrintToServer("mashup_einfo: %d invalid", e);
        return Plugin_Handled;
    }
    Describe(e);
    return Plugin_Handled;
}

public Action Command_Find(int args)
{
    char cls[64];
    char sub[128] = "";
    GetCmdArg(1, cls, sizeof(cls));
    if (args >= 2) {
        GetCmdArg(2, sub, sizeof(sub));
    }
    int e = -1;
    char model[PLATFORM_MAX_PATH];
    while ((e = FindEntityByClassname(e, cls)) != -1) {
        if (sub[0] != '\0') {
            model[0] = '\0';
            if (HasEntProp(e, Prop_Data, "m_ModelName")) {
                GetEntPropString(e, Prop_Data, "m_ModelName", model, sizeof(model));
            }
            if (StrContains(model, sub, false) == -1) {
                continue;
            }
        }
        Describe(e);
    }
    return Plugin_Handled;
}

public Action Command_Hp(int args)
{
    int e = GetCmdArgInt(1);
    int hp = GetCmdArgInt(2);
    if (e >= 1 && e <= MaxClients) {
        if (!IsPlayerAlive(e)) {
            CS_RespawnPlayer(e);
        }
        SetEntityHealth(e, hp);
        // Optional armour and helmet (players only).
        if (args >= 3) {
            SetEntProp(e, Prop_Send, "m_ArmorValue", GetCmdArgInt(3));
            SetEntProp(e, Prop_Send, "m_bHasHelmet", args >= 4 ? GetCmdArgInt(4) : 0);
        }
    } else {
        SetEntProp(e, Prop_Data, "m_iHealth", hp);
    }
    return Plugin_Handled;
}

public Action Command_Tp(int args)
{
    int e = GetCmdArgInt(1);
    float p[3];
    float v[3];
    for (int i = 0; i < 3; i++) {
        p[i] = GetCmdArgFloat(2 + i);
    }
    if (args >= 7) {
        for (int i = 0; i < 3; i++) {
            v[i] = GetCmdArgFloat(5 + i);
        }
        TeleportEntity(e, p, NULL_VECTOR, v);
    } else {
        TeleportEntity(e, p, NULL_VECTOR, NULL_VECTOR);
    }
    return Plugin_Handled;
}

public Action Command_Hurt(int args)
{
    int victim = GetCmdArgInt(1);
    int attacker = GetCmdArgInt(2);
    float dmg = GetCmdArgFloat(3);
    int type = GetCmdArgInt(4);
    if (attacker < 0) {
        attacker = 0;
    }
    if (args >= 7) {
        float pos[3];
        for (int i = 0; i < 3; i++) {
            pos[i] = GetCmdArgFloat(5 + i);
        }
        SDKHooks_TakeDamage(victim, attacker, attacker, dmg, type, -1, NULL_VECTOR, pos);
    } else {
        SDKHooks_TakeDamage(victim, attacker, attacker, dmg, type);
    }
    return Plugin_Handled;
}

public Action Command_Fire(int args)
{
    int c = GetClientOfUserId(GetCmdArgInt(1));
    if (c == 0) {
        PrintToServer("mashup_fire: no such user");
        return Plugin_Handled;
    }
    for (int i = 0; i < 3; i++) {
        g_firePos[i] = GetCmdArgFloat(2 + i);
    }
    g_fireAng[0] = GetCmdArgFloat(5);
    g_fireAng[1] = GetCmdArgFloat(6);
    g_fireAng[2] = 0.0;
    char weapon[64];
    GetCmdArg(7, weapon, sizeof(weapon));
    int b = GetCmdArgInt(8);
    g_fireButtons = ((b & 1) ? IN_ATTACK : 0) | ((b & 2) ? IN_ATTACK2 : 0);
    g_fireStart = GetCmdArgInt(9);
    g_fireTicks = GetCmdArgInt(10);
    if (!IsPlayerAlive(c)) {
        CS_RespawnPlayer(c);
    }
    for (int slot = 0; slot < 5; slot++) {
        int w;
        while ((w = GetPlayerWeaponSlot(c, slot)) != -1) {
            RemovePlayerItem(c, w);
            AcceptEntityInput(w, "Kill");
        }
    }
    int w = GivePlayerItem(c, weapon);
    if (w != -1) {
        EquipPlayerWeapon(c, w);
        SetEntPropEnt(c, Prop_Send, "m_hActiveWeapon", w);
    }
    float zero[3];
    TeleportEntity(c, g_firePos, g_fireAng, zero);
    g_fireClient = c;
    g_fireTick = 0;
    PrintToServer("mashup_fire: client %d", c);
    return Plugin_Handled;
}

public Action OnPlayerRunCmd(int client, int &buttons, int &impulse, float vel[3], float angles[3], int &weapon,
    int &subtype, int &cmdnum, int &tickcount, int &seed, int mouse[2])
{
    if (client != g_fireClient || g_fireClient == 0) {
        return Plugin_Continue;
    }
    if (g_fireTick > g_fireStart + g_fireTicks + 200) {
        g_fireClient = 0;
        return Plugin_Continue;
    }
    angles[0] = g_fireAng[0];
    angles[1] = g_fireAng[1];
    angles[2] = 0.0;
    SetEntPropVector(client, Prop_Data, "v_angle", angles);
    float zero[3];
    TeleportEntity(client, g_firePos, angles, zero);
    vel[0] = 0.0;
    vel[1] = 0.0;
    vel[2] = 0.0;
    weapon = 0;
    buttons = 0;
    if (g_fireTick >= g_fireStart && g_fireTick < g_fireStart + g_fireTicks) {
        buttons = g_fireButtons;
        Log("fire %d tick %d", client, g_fireTick);
    }
    g_fireTick++;
    return Plugin_Changed;
}

public Action Command_Kill(int args)
{
    int e = GetCmdArgInt(1);
    if (e > MaxClients && IsValidEntity(e)) {
        AcceptEntityInput(e, "Kill");
    }
    return Plugin_Handled;
}

public Action Command_Input(int args)
{
    int e = GetCmdArgInt(1);
    char input[64];
    char param[128];
    GetCmdArg(2, input, sizeof(input));
    int activator = -1;
    if (args >= 3) {
        GetCmdArg(3, param, sizeof(param));
        SetVariantString(param);
    }
    if (args >= 4) {
        activator = GetCmdArgInt(4);
    }
    bool ok = AcceptEntityInput(e, input, activator, activator);
    PrintToServer("mashup_input: %d", ok);
    return Plugin_Handled;
}

int g_watch = -1;
int g_watchFrames;

public Action Command_Watch(int args)
{
    int e = GetCmdArgInt(1);
    g_watch = IsValidEntity(e) ? EntIndexToEntRef(e) : -1;
    g_watchFrames = GetCmdArgInt(2);
    return Plugin_Handled;
}

public void OnGameFrame()
{
    if (g_watch == -1 || g_watchFrames <= 0) {
        return;
    }
    g_watchFrames--;
    int e = EntRefToEntIndex(g_watch);
    if (e == INVALID_ENT_REFERENCE) {
        Log("watch gone");
        g_watch = -1;
        return;
    }
    int effects = HasEntProp(e, Prop_Send, "m_fEffects") ? GetEntProp(e, Prop_Send, "m_fEffects") : -1;
    int solid = HasEntProp(e, Prop_Send, "m_nSolidType") ? GetEntProp(e, Prop_Send, "m_nSolidType") : -1;
    int sflags = HasEntProp(e, Prop_Send, "m_usSolidFlags") ? GetEntProp(e, Prop_Send, "m_usSolidFlags") : -1;
    int render = HasEntProp(e, Prop_Send, "m_nRenderMode") ? GetEntProp(e, Prop_Send, "m_nRenderMode") : -1;
    int takedmg = HasEntProp(e, Prop_Data, "m_takedamage") ? GetEntProp(e, Prop_Data, "m_takedamage") : -1;
    Log("watch %d hp=%d effects=%d solid=%d sflags=%d render=%d takedmg=%d", e, Health(e), effects, solid, sflags, render, takedmg);
}

public Action Command_Panes(int args)
{
    int e = GetCmdArgInt(1);
    int wide = GetEntProp(e, Prop_Send, "m_nNumWide");
    int high = GetEntProp(e, Prop_Send, "m_nNumHigh");
    int broken = GetEntProp(e, Prop_Send, "m_bIsBroken");
    char row[300];
    PrintToServer("panes %d x %d broken %d solid %d sflags %d", wide, high, broken, GetEntProp(e, Prop_Send, "m_nSolidType"),
        GetEntProp(e, Prop_Send, "m_usSolidFlags"));
    for (int h = high - 1; h >= 0; h--) {
        int n = 0;
        for (int w = 0; w < wide && n < sizeof(row) - 1; w++) {
            row[n++] = GetEntProp(e, Prop_Send, "m_RawPanelBitVec", 1, w + h * wide) ? '#' : '.';
        }
        row[n] = '\0';
        PrintToServer("%s", row);
    }
    return Plugin_Handled;
}

public Action Command_GetProp(int args)
{
    int e = GetCmdArgInt(1);
    char name[64];
    GetCmdArg(2, name, sizeof(name));
    if (e == -1) {
        PrintToServer("gamerules %s %d", name, GameRules_GetProp(name));
        return Plugin_Handled;
    }
    if (!HasEntProp(e, Prop_Data, name)) {
        PrintToServer("no %s", name);
        return Plugin_Handled;
    }
    if (args >= 3) {
        PrintToServer("%s %f", name, GetEntPropFloat(e, Prop_Data, name));
    } else {
        PrintToServer("%s %d", name, GetEntProp(e, Prop_Data, name));
    }
    return Plugin_Handled;
}
