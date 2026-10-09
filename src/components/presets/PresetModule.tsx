import React, { useState } from 'react';
import { Download, Upload, Package, Check, AlertTriangle, Layers, ExternalLink, ShieldCheck, Sparkles } from 'lucide-react';
import { ModInfo, StudioPathsUI, ModPreset, MissingModsReport } from '../../types';
import { TauriService } from '../../services/tauri';
import { InlineError } from '../common/InlineError';

interface PresetModuleProps {
  paths: StudioPathsUI;
  mods: ModInfo[];
  onApplyPresetLoadOrder: (presetLoadOrder: string[], activeIds: string[]) => void;
}

export const PresetModule: React.FC<PresetModuleProps> = ({
  paths,
  mods,
  onApplyPresetLoadOrder,
}) => {
  const [presetName, setPresetName] = useState('');
  const [presetDesc, setPresetDesc] = useState('');
  const [importedPreset, setImportedPreset] = useState<ModPreset | null>(null);
  const [missingReport, setMissingReport] = useState<MissingModsReport | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  /** Inline replacement for the old alert() calls; null renders nothing. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const activeMods = mods.filter((m) => m.enabled);

  const handleExportPreset = async () => {
    setErrorMessage(null);
    if (!presetName.trim()) {
      setErrorMessage('Give the collection a name before saving it.');
      return;
    }

    setIsExporting(true);
    try {
      const filePath = await TauriService.pickSaveFile(`${presetName.toLowerCase().replace(/\s+/g, '_')}.pzpack`);
      if (filePath) {
        const preset: ModPreset = {
          id: `preset_${Date.now()}`,
          name: presetName.trim(),
          description: presetDesc.trim() || undefined,
          author: 'PZ Mod Studio User',
          created_at: new Date().toISOString(),
          mods: activeMods.map((m) => ({
            mod_id: m.mod_id,
            name: m.name,
            workshop_id: m.workshop_id,
            enabled: true,
          })),
          load_order: mods.map((m) => m.mod_id),
        };

        await TauriService.exportPresetFile(preset, filePath);
        setStatusMessage(`✨ Collection saved successfully at: ${filePath}`);
        setPresetName('');
        setPresetDesc('');
      }
    } catch (err: any) {
      setErrorMessage(`Could not save the collection: ${err?.message ? err.message : err}`);
    } finally {
      setIsExporting(false);
    }
  };

  const handleImportPreset = async () => {
    setErrorMessage(null);
    try {
      const filePath = await TauriService.pickOpenFile();
      if (filePath) {
        const preset: ModPreset = await TauriService.importPresetFile(filePath);
        setImportedPreset(preset);

        const report: MissingModsReport = await TauriService.checkMissingPresetMods(
          preset,
          paths.user_zomboid_dir,
          paths.workshop_dir
        );
        setMissingReport(report);
      }
    } catch (err: any) {
      setErrorMessage(`Could not import that .pzpack file: ${err?.message ? err.message : err}`);
    }
  };

  const handleApplyPreset = () => {
    if (!importedPreset) return;
    const activeIds = importedPreset.mods.map((m) => m.mod_id);
    onApplyPresetLoadOrder(importedPreset.load_order, activeIds);
    setStatusMessage(`✨ Collection '${importedPreset.name}' applied successfully.`);
    setImportedPreset(null);
    setMissingReport(null);
  };

  return (
    <div className="space-y-6">
      {/* Top Banner Header */}
      <div className="p-5 bg-gradient-to-r from-cyan-950/80 via-slate-900 to-indigo-950/80 border border-cyan-800/60 rounded-2xl shadow-xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Package className="w-6 h-6 text-cyan-400" />
            <h2 className="text-xl font-bold text-slate-100 tracking-tight">
              Presets & Collections (.pzpack)
            </h2>
          </div>
          <p className="text-xs text-slate-300 max-w-2xl leading-relaxed">
            Save, share, and restore complete mod collections with their exact load order and pre-configured compatibility patches.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={handleImportPreset}
            className="px-4 py-2.5 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white rounded-xl text-xs font-bold transition shadow-lg flex items-center gap-2 cursor-pointer"
          >
            <Upload className="w-4 h-4" />
            <span>Import .pzpack</span>
          </button>
        </div>
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
        title="Presets error"
        message={errorMessage}
        onDismiss={() => setErrorMessage(null)}
        onRetry={handleImportPreset}
        retryLabel="Try import again"
      />

      {/* Main Grid: Export Panel & Import Viewer */}
      <div className="grid grid-cols-12 gap-6">
        {/* Left Column: Export Current Modpack */}
        <div className="col-span-12 lg:col-span-6 bg-slate-900/80 border border-slate-800 rounded-2xl p-5 space-y-4 shadow-lg flex flex-col justify-between">
          <div className="space-y-4">
            <div className="border-b border-slate-800 pb-3 space-y-1">
              <h3 className="text-sm font-bold text-slate-200 flex items-center gap-2">
                <Download className="w-4 h-4 text-cyan-400" />
                <span>Export Current Collection</span>
              </h3>
              <p className="text-xs text-slate-400">
                Package your <b className="text-emerald-400">{activeMods.length} active mods</b> and load order into a portable <code className="text-cyan-300">.pzpack</code> file.
              </p>
            </div>

            <div className="space-y-3">
              <div>
                <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                  Collection Name
                </label>
                <input
                  type="text"
                  placeholder="e.g. Hardcore Survival Pack B42"
                  value={presetName}
                  onChange={(e) => setPresetName(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-cyan-500 font-medium"
                />
              </div>

              <div>
                <label className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block mb-1">
                  Description (Optional)
                </label>
                <textarea
                  rows={3}
                  placeholder="Modpack summary, map recommendations, or notes for your friends..."
                  value={presetDesc}
                  onChange={(e) => setPresetDesc(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-200 focus:outline-none focus:border-cyan-500 font-sans"
                />
              </div>

              <div className="p-3 bg-slate-950/90 border border-slate-800 rounded-xl space-y-1 font-mono text-[11px]">
                <div className="flex items-center justify-between text-slate-300">
                  <span>Included Active Mods:</span>
                  <span className="text-emerald-400 font-bold">{activeMods.length}</span>
                </div>
                <div className="flex items-center justify-between text-slate-300">
                  <span>Total in Load Order:</span>
                  <span className="text-cyan-400 font-bold">{mods.length}</span>
                </div>
              </div>
            </div>
          </div>

          <button
            onClick={handleExportPreset}
            disabled={isExporting || activeMods.length === 0}
            className="w-full py-3 bg-gradient-to-r from-cyan-600 to-blue-600 hover:from-cyan-500 hover:to-blue-500 disabled:opacity-40 text-white rounded-xl text-xs font-bold transition cursor-pointer shadow-lg flex items-center justify-center gap-2 mt-4"
          >
            <Download className="w-4 h-4" />
            <span>Save Collection (.pzpack)</span>
          </button>
        </div>

        {/* Right Column: Import & Missing Mods Viewer */}
        <div className="col-span-12 lg:col-span-6 bg-slate-900/80 border border-slate-800 rounded-2xl p-5 space-y-4 shadow-lg flex flex-col justify-between">
          {importedPreset ? (
            <div className="space-y-4 flex-1 flex flex-col justify-between">
              <div className="space-y-3">
                <div className="border-b border-slate-800 pb-3 flex items-start justify-between">
                  <div>
                    <h3 className="text-sm font-bold text-cyan-300 flex items-center gap-2">
                      <Sparkles className="w-4 h-4 text-cyan-400" />
                      <span>{importedPreset.name}</span>
                    </h3>
                    <p className="text-xs text-slate-400 mt-0.5">
                      {importedPreset.description || 'No description provided.'}
                    </p>
                  </div>
                  <span className="text-[10px] font-mono bg-cyan-950 text-cyan-300 px-2 py-0.5 rounded border border-cyan-800">
                    {importedPreset.mods.length} Mods
                  </span>
                </div>

                {/* Missing Mods Diagnostic Card */}
                {missingReport && (
                  <div className="space-y-2">
                    {missingReport.missing_mods.length > 0 ? (
                      <div className="p-3.5 bg-amber-950/70 border-2 border-amber-500/80 rounded-xl space-y-2 text-xs shadow-md">
                        <div className="flex items-center justify-between font-bold text-amber-300">
                          <div className="flex items-center gap-2">
                            <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
                            <span>Mods Not Installed on your Disk ({missingReport.missing_mods.length})</span>
                          </div>
                        </div>
                        <p className="text-[11px] text-slate-300 leading-relaxed">
                          To fully enjoy this pack, you must subscribe to the following mods on the Steam Workshop:
                        </p>
                        <div className="max-h-36 overflow-y-auto space-y-1 pr-1">
                          {missingReport.missing_mods.map((m, idx) => (
                            <div key={idx} className="flex items-center justify-between p-1.5 bg-slate-950/80 rounded border border-slate-800 text-[11px] font-mono">
                              <span className="text-slate-200 truncate max-w-[220px]">{m.name}</span>
                              {m.workshop_id ? (
                                <button
                                  onClick={() => TauriService.openExternalUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${m.workshop_id}`)}
                                  className="text-cyan-400 hover:underline flex items-center gap-1 text-[10px]"
                                >
                                  <span>Open Steam (#{m.workshop_id})</span>
                                  <ExternalLink className="w-3 h-3" />
                                </button>
                              ) : (
                                <span className="text-slate-500 text-[10px]">Local Mod</span>
                              )}
                            </div>
                          ))}
                        </div>
                      </div>
                    ) : (
                      <div className="p-3.5 bg-emerald-950/60 border border-emerald-700/80 rounded-xl flex items-center gap-3 text-xs text-emerald-300">
                        <ShieldCheck className="w-5 h-5 text-emerald-400 shrink-0" />
                        <div>
                          <div className="font-bold">All mods are installed on your PC!</div>
                          <div className="text-[11px] text-slate-300">You can apply the load order immediately.</div>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>

              <div className="flex items-center gap-3 pt-2">
                <button
                  onClick={handleApplyPreset}
                  className="flex-1 py-3 bg-emerald-600 hover:bg-emerald-500 text-white rounded-xl text-xs font-bold transition cursor-pointer shadow-lg flex items-center justify-center gap-2"
                >
                  <Check className="w-4 h-4" />
                  <span>Apply Collection to my Game</span>
                </button>
                <button
                  onClick={() => {
                    setImportedPreset(null);
                    setMissingReport(null);
                  }}
                  className="px-4 py-3 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl text-xs font-bold transition cursor-pointer"
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <div className="h-full flex flex-col items-center justify-center p-8 text-center space-y-3">
              <div className="w-14 h-14 rounded-2xl bg-slate-800/80 border border-slate-700/60 flex items-center justify-center shadow">
                <Layers className="w-7 h-7 text-cyan-400" />
              </div>
              <div className="space-y-1 max-w-md">
                <h3 className="text-sm font-bold text-slate-200">No preset loaded</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Click on <b className="text-cyan-400">Import .pzpack</b> above to open a saved collection and verify which mods you need to subscribe to.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
