import React, { useState, useEffect } from 'react';
import { Layers, Plus, Check, Trash2, Zap, FolderArchive, RefreshCw, Edit3, X, Save } from 'lucide-react';
import { ModInfo, StudioPathsUI, AppInstance } from '../../types';
import { TauriService } from '../../services/tauri';
import { InlineError } from '../common/InlineError';

interface InstanceModuleProps {
  paths: StudioPathsUI;
  mods: ModInfo[];
  onApplyInstanceLoadOrder: (loadOrder: string[], activeIds: string[]) => void;
  onProfileActivated?: (inst: AppInstance) => void;
}

export const InstanceModule: React.FC<InstanceModuleProps> = ({
  paths,
  mods,
  onApplyInstanceLoadOrder,
  onProfileActivated,
}) => {
  const [instances, setInstances] = useState<AppInstance[]>([]);
  const [newInstanceName, setNewInstanceName] = useState('');
  const [newInstanceDesc, setNewInstanceDesc] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  /** Inline replacement for the old alert() calls; null renders nothing. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Edit Modal State
  const [editingInstance, setEditingInstance] = useState<AppInstance | null>(null);
  const [editName, setEditName] = useState('');
  const [editDesc, setEditDesc] = useState('');

  const activeMods = mods.filter((m) => m.enabled);

  const loadInstances = async () => {
    setIsLoading(true);
    try {
      const list: AppInstance[] = await TauriService.listInstances(paths.user_zomboid_dir);
      setInstances(list);
    } catch (err) {
      console.error('Error loading profiles:', err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadInstances();
  }, [paths.user_zomboid_dir]);

  const handleCreateInstance = async () => {
    setErrorMessage(null);
    if (!newInstanceName.trim()) {
      setErrorMessage('Give the profile a name before creating it.');
      return;
    }

    try {
      const activeModIds = activeMods.map((m) => m.mod_id);
      const loadOrder = mods.map((m) => m.mod_id);

      const created: AppInstance = await TauriService.createInstance(
        paths.user_zomboid_dir || '',
        newInstanceName.trim(),
        newInstanceDesc.trim() || undefined,
        activeModIds,
        loadOrder
      );

      setNewInstanceName('');
      setNewInstanceDesc('');
      setStatusMessage(`✨ Profile '${created.name}' created and activated successfully.`);
      await loadInstances();

      // Automatically activate the new profile so all tabs unlock immediately
      await handleActivateInstance(created);
    } catch (err: any) {
      setErrorMessage(`Could not create the profile: ${err?.message ? err.message : err}`);
    }
  };

  const handleOpenEdit = (inst: AppInstance) => {
    setEditingInstance(inst);
    setEditName(inst.name);
    setEditDesc(inst.description || '');
  };

  const handleSaveEdit = async () => {
    if (!editingInstance || !editName.trim()) return;

    try {
      const updated: AppInstance = {
        ...editingInstance,
        name: editName.trim(),
        description: editDesc.trim() || undefined,
      };

      await TauriService.updateInstance(paths.user_zomboid_dir, updated);
      setStatusMessage(`✨ Profile '${updated.name}' updated successfully.`);
      setEditingInstance(null);
      await loadInstances();
    } catch (err: any) {
      setErrorMessage(`Could not save the profile changes: ${err?.message ? err.message : err}`);
    }
  };

  const handleActivateInstance = async (inst: AppInstance) => {
    setErrorMessage(null);
    try {
      await TauriService.activateInstance(paths.user_zomboid_dir, inst.id);
      onApplyInstanceLoadOrder(inst.load_order, inst.active_mod_ids);
      if (onProfileActivated) {
        onProfileActivated(inst);
      }
      setStatusMessage(`⚡ Profile '${inst.name}' activated successfully. Mods applied to the game!`);
      await loadInstances();
    } catch (err: any) {
      setErrorMessage(
        `Could not activate '${inst.name}': ${err?.message ? err.message : err}. Your previous profile is still active.`
      );
    }
  };

  const handleDeleteInstance = async (inst: AppInstance) => {
    if (!confirm(`Delete profile '${inst.name}'?`)) return;
    try {
      await TauriService.deleteInstance(paths.user_zomboid_dir, inst.id);
      setStatusMessage(`Profile '${inst.name}' deleted.`);
      await loadInstances();
    } catch (err: any) {
      setErrorMessage(`Could not delete '${inst.name}': ${err?.message ? err.message : err}`);
    }
  };

  return (
    <div className="space-y-6">
      {/* Top Banner */}
      <div className="p-5 bg-gradient-to-r from-emerald-950/80 via-slate-900 to-teal-950/80 border border-emerald-800/60 rounded-2xl shadow-xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Layers className="w-6 h-6 text-emerald-400" />
            <h2 className="text-xl font-bold text-slate-100 tracking-tight">
              Mod Profiles
            </h2>
            {isLoading && <RefreshCw className="w-4 h-4 text-emerald-400 animate-spin ml-2" />}
          </div>
          <p className="text-xs text-slate-300 max-w-2xl leading-relaxed">
            Create and save custom mod combinations (e.g., <i>Vanilla Plus B42</i>, <i>Brita + Weapons</i>, <i>Co-op Server</i>) and switch between them with 1 click without losing your load order.
          </p>
        </div>

        <span className="text-xs font-mono bg-emerald-950 text-emerald-300 border border-emerald-800 px-3 py-1.5 rounded-xl font-bold">
          {instances.length} Saved Profiles
        </span>
      </div>

      {statusMessage && (
        <div className="p-3 bg-emerald-950/80 border border-emerald-700/80 rounded-xl text-xs text-emerald-300 font-bold flex items-center justify-between">
          <span>{statusMessage}</span>
          <button onClick={() => setStatusMessage(null)} className="text-emerald-400 hover:underline">
            Close
          </button>
        </div>
      )}

      <InlineError
        title="Mod Profiles error"
        message={errorMessage}
        onDismiss={() => setErrorMessage(null)}
        onRetry={loadInstances}
        retryLabel="Reload profiles"
      />

      {/* Main Grid: Create Profile & Instance Cards */}
      <div className="grid grid-cols-12 gap-6">
        {/* Left Column: Create New Profile */}
        <div className="col-span-12 lg:col-span-4 bg-slate-900/80 border border-slate-800 rounded-2xl p-5 space-y-4 shadow-lg">
          <div className="border-b border-slate-800 pb-3 space-y-1">
            <h3 className="text-sm font-bold text-slate-200 flex items-center gap-2">
              <Plus className="w-4 h-4 text-emerald-400" />
              <span>Create New Profile</span>
            </h3>
            <p className="text-xs text-slate-400">
              Create an independent profile to organize and save a mod combination.
            </p>
          </div>

          <div className="space-y-3">
            <div>
              <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                Profile Name
              </label>
              <input
                type="text"
                placeholder="e.g., B42 Brita & GunFighter"
                value={newInstanceName}
                onChange={(e) => setNewInstanceName(e.target.value)}
                className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-emerald-500 font-medium"
              />
            </div>

            <div>
              <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                Description (Optional)
              </label>
              <textarea
                rows={3}
                placeholder="Notes for the mods included in this profile..."
                value={newInstanceDesc}
                onChange={(e) => setNewInstanceDesc(e.target.value)}
                className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-emerald-500 font-sans"
              />
            </div>

            <button
              onClick={handleCreateInstance}
              disabled={!newInstanceName.trim()}
              className="w-full py-3 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 disabled:opacity-40 text-white rounded-xl text-xs font-bold transition cursor-pointer shadow-lg flex items-center justify-center gap-2 mt-2"
            >
              <Plus className="w-4 h-4" />
              <span>Create New Profile</span>
            </button>
          </div>
        </div>

        {/* Right Column: Display Cards of All Saved Profiles */}
        <div className="col-span-12 lg:col-span-8 space-y-4">
          {instances.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {instances.map((inst) => (
                <div
                  key={inst.id}
                  className={`p-4 rounded-2xl border transition flex flex-col justify-between space-y-4 shadow-lg ${
                    inst.is_active
                      ? 'bg-gradient-to-br from-emerald-950/80 via-slate-900 to-slate-900 border-emerald-500/90 ring-1 ring-emerald-500/50'
                      : 'bg-slate-900/80 border-slate-800 hover:border-slate-700'
                  }`}
                >
                  <div className="space-y-2">
                    <div className="flex items-start justify-between">
                      <div className="flex items-center gap-2">
                        <FolderArchive className={`w-5 h-5 ${inst.is_active ? 'text-emerald-400' : 'text-slate-400'}`} />
                        <h4 className="text-sm font-bold text-slate-100">{inst.name}</h4>
                      </div>

                      {inst.is_active ? (
                        <span className="text-[10px] font-mono font-bold bg-emerald-950 text-emerald-300 border border-emerald-700 px-2 py-0.5 rounded-full flex items-center gap-1">
                          <Check className="w-3 h-3" /> Active
                        </span>
                      ) : (
                        <span className="text-[10px] font-mono text-slate-500">Inactive</span>
                      )}
                    </div>

                    <p className="text-xs text-slate-300 line-clamp-2 leading-relaxed">
                      {inst.description || 'Mod profile for Project Zomboid.'}
                    </p>

                    <div className="flex items-center gap-2 text-[11px] font-mono text-slate-400 pt-1">
                      <span className="text-emerald-400 font-bold">{inst.active_mod_ids.length} Active Mods</span>
                      <span>•</span>
                      <span>Total: {inst.load_order.length}</span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 pt-2 border-t border-slate-800/80">
                    {!inst.is_active && (
                      <button
                        onClick={() => handleActivateInstance(inst)}
                        className="flex-1 py-2 bg-emerald-600 hover:bg-emerald-500 text-white rounded-xl text-xs font-bold transition flex items-center justify-center gap-1.5 cursor-pointer shadow"
                      >
                        <Zap className="w-3.5 h-3.5 fill-white" />
                        <span>Activate (1-Click)</span>
                      </button>
                    )}

                    <button
                      onClick={() => handleOpenEdit(inst)}
                      className="p-2 bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 rounded-xl transition cursor-pointer"
                      title="Edit profile"
                    >
                      <Edit3 className="w-3.5 h-3.5" />
                    </button>

                    <button
                      onClick={() => handleDeleteInstance(inst)}
                      className="p-2 bg-slate-800 hover:bg-rose-950 text-slate-400 hover:text-rose-400 border border-slate-700 hover:border-rose-700 rounded-xl transition cursor-pointer"
                      title="Delete profile"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="h-64 bg-slate-900/80 border border-slate-800 rounded-2xl flex flex-col items-center justify-center p-8 text-center space-y-3">
              <div className="w-14 h-14 rounded-2xl bg-slate-800/80 border border-slate-700/60 flex items-center justify-center shadow">
                <Layers className="w-7 h-7 text-emerald-400" />
              </div>
              <div className="space-y-1 max-w-md">
                <h3 className="text-sm font-bold text-slate-200">No profiles created</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Create your first profile in the left panel to save the current mod combination.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Edit Profile Modal */}
      {editingInstance && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl max-w-md w-full p-6 space-y-4 shadow-2xl animate-in fade-in zoom-in-95">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                <Edit3 className="w-4 h-4 text-emerald-400" />
                <span>Edit Mod Profile</span>
              </h3>
              <button
                onClick={() => setEditingInstance(null)}
                className="text-slate-400 hover:text-slate-200 p-1"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="space-y-3">
              <div>
                <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                  Profile Name
                </label>
                <input
                  type="text"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-emerald-500 font-medium"
                />
              </div>

              <div>
                <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                  Description
                </label>
                <textarea
                  rows={3}
                  value={editDesc}
                  onChange={(e) => setEditDesc(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-emerald-500 font-sans"
                />
              </div>
            </div>

            <div className="flex items-center gap-2 pt-2">
              <button
                onClick={handleSaveEdit}
                className="flex-1 py-2.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-xl text-xs font-bold transition flex items-center justify-center gap-2 cursor-pointer shadow"
              >
                <Save className="w-4 h-4" />
                <span>Save Changes</span>
              </button>
              <button
                onClick={() => setEditingInstance(null)}
                className="px-4 py-2.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-xs font-bold transition cursor-pointer"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

