// FM27 Trainer · Profesyonel Akıllı Kurulum Sihirbazı (Setup.exe)
// - Özel koyu temalı profesyonel kurulum arayüzü ve gömülü Holyman3 logosu
// - İlk ekranda kurulum ve uygulama dilini seçtirir (Türkçe / English / Português)
// - Klasör Seç (Gözat) butonu, özel çizimli modern ilerleme çubuğu ve durum rozeti
// - İnternet bağlantısı varsa GitHub Releases üzerinden daha yeni bir sürüm olup olmadığını kontrol eder;
//   yoksa veya çevrimdışıysa kendi içindeki gömülü v1.3.0 sürümünü kurar.

using System;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Net;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Forms;
using Microsoft.Win32;

public class ModernProgressBar : Control
{
    private int val = 0;
    public int Value
    {
        get { return val; }
        set { val = Math.Max(0, Math.Min(100, value)); Invalidate(); }
    }

    public ModernProgressBar()
    {
        this.SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer, true);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        Graphics g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        Rectangle rect = new Rectangle(0, 0, this.Width - 1, this.Height - 1);
        using (SolidBrush bg = new SolidBrush(Color.FromArgb(20, 24, 34)))
        using (Pen border = new Pen(Color.FromArgb(42, 48, 66)))
        {
            g.FillRectangle(bg, rect);
            g.DrawRectangle(border, rect);
        }
        if (val > 0)
        {
            int fillW = (int)((this.Width - 2) * (val / 100.0));
            if (fillW > 2)
            {
                Rectangle fillRect = new Rectangle(1, 1, fillW, this.Height - 2);
                using (LinearGradientBrush lg = new LinearGradientBrush(fillRect, Color.FromArgb(124, 92, 255), Color.FromArgb(46, 213, 115), LinearGradientMode.Horizontal))
                {
                    g.FillRectangle(lg, fillRect);
                }
            }
        }
    }
}

public class SetupForm : Form
{
    public const string BUNDLED_VERSION = "1.3.0";

    private static bool IsRemoteNewer(string remoteTag, string localVer)
    {
        try
        {
            string rClean = remoteTag.Trim().TrimStart('v', 'V');
            string lClean = localVer.Trim().TrimStart('v', 'V');
            string[] rParts = rClean.Split('.', '-', '_');
            string[] lParts = lClean.Split('.', '-', '_');
            int len = Math.Max(rParts.Length, lParts.Length);
            for (int i = 0; i < len; i++)
            {
                int rv = 0, lv = 0;
                if (i < rParts.Length) int.TryParse(rParts[i], out rv);
                if (i < lParts.Length) int.TryParse(lParts[i], out lv);
                if (rv > lv) return true;
                if (rv < lv) return false;
            }
        }
        catch { }
        return false;
    }

    private Panel headerPanel;
    private PictureBox logoBox;
    private Label titleLbl;
    private Label badgeLbl;
    private Label subLbl;
    private Label featLbl;

    private Panel cardPanel;
    private Label langLbl;
    private ComboBox langBox;
    private Label pathLbl;
    private TextBox pathBox;
    private Button browseBtn;
    private Label graphicsLbl;
    private TextBox graphicsBox;
    private Button graphicsBrowseBtn;
    private CheckBox desktopChk;
    private CheckBox launchChk;

    private ModernProgressBar progressBar;
    private Label statusLbl;
    private Label footerLbl;
    private Button installBtn;

    private static string DetectDefaultGraphicsFolder()
    {
        try
        {
            string docs = Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments);
            if (string.IsNullOrEmpty(docs))
            {
                string prof = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                docs = Path.Combine(prof, "Documents");
            }
            string p27 = Path.Combine(docs, "Sports Interactive", "Football Manager 27", "graphics");
            if (Directory.Exists(p27)) return p27;
            string p26 = Path.Combine(docs, "Sports Interactive", "Football Manager 26", "graphics");
            if (Directory.Exists(p26)) return p26;
            return p27;
        }
        catch
        {
            return "";
        }
    }

    public SetupForm()
    {
        this.Text = "FM27 Trainer Setup v" + BUNDLED_VERSION + " · Holyman3";
        this.ClientSize = new Size(580, 505);
        this.FormBorderStyle = FormBorderStyle.FixedDialog;
        this.MaximizeBox = false;
        this.StartPosition = FormStartPosition.CenterScreen;
        this.BackColor = Color.FromArgb(13, 15, 21);
        this.ForeColor = Color.FromArgb(238, 242, 250);
        this.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
        this.DoubleBuffered = true;

        try
        {
            this.Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
        }
        catch { }

        // ---- ÜST BANNER (LOGO + BAŞLIK + SÜRÜM ROZETİ) ----
        headerPanel = new Panel();
        headerPanel.Location = new Point(0, 0);
        headerPanel.Size = new Size(580, 108);
        headerPanel.BackColor = Color.FromArgb(19, 23, 33);
        headerPanel.Paint += delegate(object s, PaintEventArgs e)
        {
            using (LinearGradientBrush accent = new LinearGradientBrush(new Rectangle(0, 0, 580, 3), Color.FromArgb(124, 92, 255), Color.FromArgb(83, 252, 24), LinearGradientMode.Horizontal))
            {
                e.Graphics.FillRectangle(accent, 0, 0, 580, 3);
            }
            using (Pen bottomLine = new Pen(Color.FromArgb(36, 42, 58)))
            {
                e.Graphics.DrawLine(bottomLine, 0, headerPanel.Height - 1, headerPanel.Width, headerPanel.Height - 1);
            }
        };
        this.Controls.Add(headerPanel);

        logoBox = new PictureBox();
        logoBox.Location = new Point(24, 20);
        logoBox.Size = new Size(68, 68);
        logoBox.SizeMode = PictureBoxSizeMode.Zoom;
        logoBox.BackColor = Color.Transparent;
        try
        {
            using (Stream ls = Assembly.GetExecutingAssembly().GetManifestResourceStream("logo.png"))
            {
                if (ls != null)
                {
                    logoBox.Image = Image.FromStream(ls);
                }
            }
        }
        catch { }
        headerPanel.Controls.Add(logoBox);

        titleLbl = new Label();
        titleLbl.Text = "FM27 Trainer";
        titleLbl.Font = new Font("Segoe UI", 18f, FontStyle.Bold);
        titleLbl.ForeColor = Color.FromArgb(245, 247, 255);
        titleLbl.Location = new Point(104, 17);
        titleLbl.Size = new Size(210, 34);
        headerPanel.Controls.Add(titleLbl);

        badgeLbl = new Label();
        badgeLbl.Text = "v" + BUNDLED_VERSION + " · by Holyman3";
        badgeLbl.Font = new Font("Segoe UI", 9f, FontStyle.Bold);
        badgeLbl.ForeColor = Color.FromArgb(168, 148, 255);
        badgeLbl.BackColor = Color.FromArgb(32, 30, 58);
        badgeLbl.TextAlign = ContentAlignment.MiddleCenter;
        badgeLbl.Location = new Point(282, 23);
        badgeLbl.Size = new Size(138, 24);
        headerPanel.Controls.Add(badgeLbl);

        subLbl = new Label();
        subLbl.Location = new Point(106, 52);
        subLbl.Size = new Size(455, 20);
        subLbl.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
        subLbl.ForeColor = Color.FromArgb(175, 185, 205);
        headerPanel.Controls.Add(subLbl);

        featLbl = new Label();
        featLbl.Location = new Point(106, 74);
        featLbl.Size = new Size(455, 18);
        featLbl.Font = new Font("Segoe UI", 8.5f, FontStyle.Regular);
        featLbl.ForeColor = Color.FromArgb(118, 235, 160);
        headerPanel.Controls.Add(featLbl);

        // ---- ORTA AYAR KARTI ----
        cardPanel = new Panel();
        cardPanel.Location = new Point(24, 124);
        cardPanel.Size = new Size(532, 268);
        cardPanel.BackColor = Color.FromArgb(20, 24, 34);
        cardPanel.Paint += delegate(object s, PaintEventArgs e)
        {
            using (Pen border = new Pen(Color.FromArgb(38, 45, 62)))
            {
                e.Graphics.DrawRectangle(border, 0, 0, cardPanel.Width - 1, cardPanel.Height - 1);
            }
        };
        this.Controls.Add(cardPanel);

        langLbl = new Label();
        langLbl.Location = new Point(20, 18);
        langLbl.Size = new Size(230, 24);
        langLbl.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
        langLbl.ForeColor = Color.FromArgb(230, 235, 248);
        cardPanel.Controls.Add(langLbl);

        langBox = new ComboBox();
        langBox.DropDownStyle = ComboBoxStyle.DropDownList;
        langBox.FlatStyle = FlatStyle.Flat;
        langBox.Location = new Point(256, 15);
        langBox.Size = new Size(254, 28);
        langBox.BackColor = Color.FromArgb(30, 36, 50);
        langBox.ForeColor = Color.White;
        langBox.Items.AddRange(new object[] { "Türkçe (TR)", "English (EN)", "Português (PT)" });
        langBox.SelectedIndex = 0;
        langBox.SelectedIndexChanged += delegate { UpdateTexts(); };
        cardPanel.Controls.Add(langBox);

        pathLbl = new Label();
        pathLbl.Location = new Point(20, 56);
        pathLbl.Size = new Size(480, 20);
        pathLbl.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
        pathLbl.ForeColor = Color.FromArgb(230, 235, 248);
        cardPanel.Controls.Add(pathLbl);

        pathBox = new TextBox();
        pathBox.Location = new Point(20, 78);
        pathBox.Size = new Size(396, 27);
        pathBox.BorderStyle = BorderStyle.FixedSingle;
        pathBox.BackColor = Color.FromArgb(14, 17, 25);
        pathBox.ForeColor = Color.FromArgb(225, 232, 248);
        pathBox.Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "FM27 Trainer");
        cardPanel.Controls.Add(pathBox);

        browseBtn = new Button();
        browseBtn.Location = new Point(422, 77);
        browseBtn.Size = new Size(88, 27);
        browseBtn.BackColor = Color.FromArgb(34, 41, 58);
        browseBtn.ForeColor = Color.FromArgb(220, 228, 245);
        browseBtn.FlatStyle = FlatStyle.Flat;
        browseBtn.FlatAppearance.BorderColor = Color.FromArgb(56, 66, 92);
        browseBtn.Font = new Font("Segoe UI", 8.75f, FontStyle.Bold);
        browseBtn.Cursor = Cursors.Hand;
        browseBtn.Click += delegate
        {
            using (FolderBrowserDialog fbd = new FolderBrowserDialog())
            {
                fbd.SelectedPath = pathBox.Text;
                if (fbd.ShowDialog() == DialogResult.OK && !string.IsNullOrEmpty(fbd.SelectedPath))
                {
                    pathBox.Text = fbd.SelectedPath;
                }
            }
        };
        cardPanel.Controls.Add(browseBtn);

        graphicsLbl = new Label();
        graphicsLbl.Location = new Point(20, 116);
        graphicsLbl.Size = new Size(480, 20);
        graphicsLbl.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
        graphicsLbl.ForeColor = Color.FromArgb(230, 235, 248);
        cardPanel.Controls.Add(graphicsLbl);

        graphicsBox = new TextBox();
        graphicsBox.Location = new Point(20, 138);
        graphicsBox.Size = new Size(396, 27);
        graphicsBox.BorderStyle = BorderStyle.FixedSingle;
        graphicsBox.BackColor = Color.FromArgb(14, 17, 25);
        graphicsBox.ForeColor = Color.FromArgb(225, 232, 248);
        graphicsBox.Text = DetectDefaultGraphicsFolder();
        cardPanel.Controls.Add(graphicsBox);

        graphicsBrowseBtn = new Button();
        graphicsBrowseBtn.Location = new Point(422, 137);
        graphicsBrowseBtn.Size = new Size(88, 27);
        graphicsBrowseBtn.BackColor = Color.FromArgb(34, 41, 58);
        graphicsBrowseBtn.ForeColor = Color.FromArgb(220, 228, 245);
        graphicsBrowseBtn.FlatStyle = FlatStyle.Flat;
        graphicsBrowseBtn.FlatAppearance.BorderColor = Color.FromArgb(56, 66, 92);
        graphicsBrowseBtn.Font = new Font("Segoe UI", 8.75f, FontStyle.Bold);
        graphicsBrowseBtn.Cursor = Cursors.Hand;
        graphicsBrowseBtn.Click += delegate
        {
            using (FolderBrowserDialog fbd = new FolderBrowserDialog())
            {
                fbd.SelectedPath = graphicsBox.Text;
                if (fbd.ShowDialog() == DialogResult.OK && !string.IsNullOrEmpty(fbd.SelectedPath))
                {
                    graphicsBox.Text = fbd.SelectedPath;
                }
            }
        };
        cardPanel.Controls.Add(graphicsBrowseBtn);

        desktopChk = new CheckBox();
        desktopChk.Location = new Point(22, 184);
        desktopChk.Size = new Size(480, 24);
        desktopChk.ForeColor = Color.FromArgb(215, 222, 238);
        desktopChk.Checked = true;
        desktopChk.Cursor = Cursors.Hand;
        cardPanel.Controls.Add(desktopChk);

        launchChk = new CheckBox();
        launchChk.Location = new Point(22, 214);
        launchChk.Size = new Size(480, 24);
        launchChk.ForeColor = Color.FromArgb(215, 222, 238);
        launchChk.Checked = true;
        launchChk.Cursor = Cursors.Hand;
        cardPanel.Controls.Add(launchChk);

        // ---- ALT İLERLEME VE AKSİYON BÖLÜMÜ ----
        progressBar = new ModernProgressBar();
        progressBar.Location = new Point(24, 406);
        progressBar.Size = new Size(532, 14);
        progressBar.Value = 0;
        this.Controls.Add(progressBar);

        statusLbl = new Label();
        statusLbl.Location = new Point(24, 426);
        statusLbl.Size = new Size(320, 22);
        statusLbl.Font = new Font("Segoe UI", 9f, FontStyle.Regular);
        statusLbl.ForeColor = Color.FromArgb(165, 175, 195);
        this.Controls.Add(statusLbl);

        footerLbl = new Label();
        footerLbl.Location = new Point(24, 458);
        footerLbl.Size = new Size(320, 22);
        footerLbl.Font = new Font("Segoe UI", 8.5f, FontStyle.Regular);
        footerLbl.ForeColor = Color.FromArgb(115, 125, 148);
        footerLbl.Text = "kick.com/holyman3  ·  Discord: Holyman3";
        this.Controls.Add(footerLbl);

        installBtn = new Button();
        installBtn.Location = new Point(356, 438);
        installBtn.Size = new Size(200, 44);
        installBtn.BackColor = Color.FromArgb(124, 92, 255);
        installBtn.ForeColor = Color.White;
        installBtn.FlatStyle = FlatStyle.Flat;
        installBtn.FlatAppearance.BorderColor = Color.FromArgb(156, 132, 255);
        installBtn.FlatAppearance.MouseOverBackColor = Color.FromArgb(138, 108, 255);
        installBtn.Font = new Font("Segoe UI", 10.5f, FontStyle.Bold);
        installBtn.Cursor = Cursors.Hand;
        installBtn.Click += delegate { RunInstall(); };
        this.Controls.Add(installBtn);

        UpdateTexts();
    }

    private string LangCode()
    {
        if (langBox.SelectedIndex == 1) return "En";
        if (langBox.SelectedIndex == 2) return "Pt";
        return "Tr";
    }

    private void UpdateTexts()
    {
        int i = langBox.SelectedIndex;
        if (i == 0)
        {
            subLbl.Text = "Football Manager 27 Antrenman, Easy Talk ve Taktik Aracı";
            featLbl.Text = "✓ Görsel Saha Dizilişi   ✓ Kayıtlı 11 Şablonları   ✓ Otomatik Easy Talk & Antrenman";
            langLbl.Text = "Uygulama ve Kurulum Dili:";
            pathLbl.Text = "Kurulum Klasörü:";
            browseBtn.Text = "Gözat…";
            graphicsLbl.Text = "FM27 Graphics Klasörü (Yüzler & Logolar - Otomatik Algılandı):";
            graphicsBrowseBtn.Text = "Gözat…";
            desktopChk.Text = "Masaüstü ve Başlat Menüsü kısayolu oluştur";
            launchChk.Text = "Kurulum tamamlandığında FM27 Trainer'ı başlat";
            installBtn.Text = "Kur ve Başlat  »";
            statusLbl.Text = "Kuruluma hazır.";
        }
        else if (i == 1)
        {
            subLbl.Text = "Football Manager 27 Training, Easy Talk & Tactics Companion";
            featLbl.Text = "✓ Visual Pitch View   ✓ Saved XI Templates   ✓ Auto Easy Talk & Training";
            langLbl.Text = "App & Installer Language:";
            pathLbl.Text = "Installation Folder:";
            browseBtn.Text = "Browse…";
            graphicsLbl.Text = "FM27 Graphics Folder (Faces & Logos - Auto-Detected):";
            graphicsBrowseBtn.Text = "Browse…";
            desktopChk.Text = "Create Desktop & Start Menu shortcuts";
            launchChk.Text = "Launch FM27 Trainer when installation completes";
            installBtn.Text = "Install & Launch  »";
            statusLbl.Text = "Ready to install.";
        }
        else
        {
            subLbl.Text = "Ferramenta de Treino, Easy Talk e Táticas para FM27";
            featLbl.Text = "✓ Vista do Campo   ✓ Modelos de 11 Inicial   ✓ Easy Talk & Treino Automático";
            langLbl.Text = "Idioma da Aplicação:";
            pathLbl.Text = "Pasta de Instalação:";
            browseBtn.Text = "Procurar…";
            graphicsLbl.Text = "Pasta Graphics do FM27 (Rostos e Logos - Auto-Detetada):";
            graphicsBrowseBtn.Text = "Procurar…";
            desktopChk.Text = "Criar atalho no Ambiente de Trabalho";
            launchChk.Text = "Iniciar FM27 Trainer ao concluir a instalação";
            installBtn.Text = "Instalar e Iniciar  »";
            statusLbl.Text = "Pronto para instalar.";
        }
    }

    private void StopRunningInstances()
    {
        try
        {
            foreach (string name in new string[] { "FM27 Trainer", "fm27-trainer" })
            {
                foreach (Process p in Process.GetProcessesByName(name))
                {
                    try { p.Kill(); p.WaitForExit(1500); } catch { }
                }
            }
        }
        catch { }
    }

    private void RunInstall()
    {
        installBtn.Enabled = false;
        langBox.Enabled = false;
        pathBox.Enabled = false;
        browseBtn.Enabled = false;
        try
        {
            StopRunningInstances();

            string targetDir = pathBox.Text.Trim();
            Directory.CreateDirectory(targetDir);
            string targetExe = Path.Combine(targetDir, "FM27 Trainer.exe");

            progressBar.Value = 25;
            statusLbl.Text = (langBox.SelectedIndex == 0) ? "Güncel sürüm kontrol ediliyor..." : "Checking for latest version...";
            Application.DoEvents();

            bool downloaded = false;
            try
            {
                ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072; // TLS 1.2
                using (WebClient wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "FM27-Trainer-Setup");
                    string json = wc.DownloadString("https://api.github.com/repos/Holymanrepo/Holyman3-Antreman-fm27/releases/latest");
                    Match tagMatch = Regex.Match(json, "\"tag_name\"\\s*:\\s*\"([^\"]+)\"");
                    string remoteTag = tagMatch.Success ? tagMatch.Groups[1].Value : "";
                    if (IsRemoteNewer(remoteTag, BUNDLED_VERSION))
                    {
                        Match m = Regex.Match(json, "\"browser_download_url\"\\s*:\\s*\"([^\"]+\\.exe)\"");
                        if (m.Success)
                        {
                            string url = m.Groups[1].Value;
                            progressBar.Value = 50;
                            statusLbl.Text = (langBox.SelectedIndex == 0) ? "En güncel sürüm indiriliyor (" + remoteTag + ")..." : "Downloading latest release (" + remoteTag + ")...";
                            Application.DoEvents();
                            string tmp = targetExe + ".tmp";
                            wc.DownloadFile(url, tmp);
                            FileInfo fi = new FileInfo(tmp);
                            if (fi.Exists && fi.Length > 500000)
                            {
                                try
                                {
                                    if (File.Exists(targetExe)) File.Delete(targetExe);
                                }
                                catch
                                {
                                    string oldExe = targetExe + ".old";
                                    if (File.Exists(oldExe)) try { File.Delete(oldExe); } catch { }
                                    try { File.Move(targetExe, oldExe); } catch { }
                                }
                                File.Move(tmp, targetExe);
                                downloaded = true;
                            }
                        }
                    }
                }
            }
            catch { }

            if (!downloaded)
            {
                progressBar.Value = 65;
                statusLbl.Text = (langBox.SelectedIndex == 0) ? "Gömülü güncel sürüm (v" + BUNDLED_VERSION + ") kuruluyor..." : "Installing bundled release (v" + BUNDLED_VERSION + ")...";
                Application.DoEvents();
                using (Stream s = Assembly.GetExecutingAssembly().GetManifestResourceStream("FM27Trainer.exe"))
                {
                    if (s != null)
                    {
                        try
                        {
                            if (File.Exists(targetExe)) File.Delete(targetExe);
                        }
                        catch
                        {
                            string oldExe = targetExe + ".old";
                            if (File.Exists(oldExe)) try { File.Delete(oldExe); } catch { }
                            try { File.Move(targetExe, oldExe); } catch { }
                        }
                        using (FileStream fs = new FileStream(targetExe, FileMode.Create, FileAccess.Write))
                        {
                            s.CopyTo(fs);
                        }
                    }
                }
            }

            progressBar.Value = 85;
            SaveInitialSettings(LangCode(), graphicsBox.Text.Trim());
            RegisterUninstaller(targetDir, targetExe);

            if (desktopChk.Checked)
            {
                string desk = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                CreateShortcut(Path.Combine(desk, "FM27 Trainer.lnk"), targetExe, targetDir);
                string startMenu = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
                CreateShortcut(Path.Combine(startMenu, "FM27 Trainer.lnk"), targetExe, targetDir);
            }

            progressBar.Value = 100;
            statusLbl.Text = (langBox.SelectedIndex == 0) ? "Kurulum tamamlandı!" : "Installation complete!";
            Application.DoEvents();

            if (launchChk.Checked && File.Exists(targetExe))
            {
                Process.Start(new ProcessStartInfo(targetExe) { WorkingDirectory = targetDir });
            }
            this.Close();
        }
        catch (Exception ex)
        {
            MessageBox.Show("Error: " + ex.Message, "FM27 Trainer Setup", MessageBoxButtons.OK, MessageBoxIcon.Error);
            installBtn.Enabled = true;
            langBox.Enabled = true;
            pathBox.Enabled = true;
            browseBtn.Enabled = true;
        }
    }

    private static string JsonEscape(string s)
    {
        if (string.IsNullOrEmpty(s)) return "";
        return s.Replace("\\", "\\\\").Replace("\"", "\\\"");
    }

    private void SaveInitialSettings(string langCode, string graphicsDir)
    {
        try
        {
            if (string.IsNullOrEmpty(graphicsDir))
            {
                graphicsDir = DetectDefaultGraphicsFolder();
            }
            string facesDir = Path.Combine(graphicsDir, "faces");
            string escGraphics = JsonEscape(graphicsDir);
            string escFaces = JsonEscape(facesDir);

            string appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string cfgDir = Path.Combine(appData, "FM27 Trainer");
            Directory.CreateDirectory(cfgDir);
            string cfgPath = Path.Combine(cfgDir, "settings.json");
            if (File.Exists(cfgPath))
            {
                string txt = File.ReadAllText(cfgPath, Encoding.UTF8);
                txt = Regex.Replace(txt, "\"lang\"\\s*:\\s*\"[A-Za-z]+\"", "\"lang\": \"" + langCode + "\"");
                if (!txt.Contains("\"lang_chosen\""))
                {
                    txt = txt.Replace("\"lang\":", "\"lang_chosen\": true,\n  \"lang\":");
                }
                else
                {
                    txt = Regex.Replace(txt, "\"lang_chosen\"\\s*:\\s*(true|false)", "\"lang_chosen\": true");
                }
                if (txt.Contains("\"graphics_dir\""))
                {
                    txt = Regex.Replace(txt, "\"graphics_dir\"\\s*:\\s*\"[^\"]*\"", "\"graphics_dir\": \"" + escGraphics + "\"");
                }
                else
                {
                    txt = txt.Replace("\"lang_chosen\": true,", "\"lang_chosen\": true,\n  \"graphics_dir\": \"" + escGraphics + "\",");
                }
                if (txt.Contains("\"faces_dir\""))
                {
                    txt = Regex.Replace(txt, "\"faces_dir\"\\s*:\\s*\"[^\"]*\"", "\"faces_dir\": \"" + escFaces + "\"");
                }
                else
                {
                    txt = txt.Replace("\"lang_chosen\": true,", "\"lang_chosen\": true,\n  \"faces_dir\": \"" + escFaces + "\",");
                }
                File.WriteAllText(cfgPath, txt, Encoding.UTF8);
            }
            else
            {
                string json = "{\n  \"lang\": \"" + langCode + "\",\n  \"lang_chosen\": true,\n  \"graphics_dir\": \"" + escGraphics + "\",\n  \"faces_dir\": \"" + escFaces + "\",\n  \"rate\": 1.17,\n  \"capture_delay\": true,\n  \"auto_training\": true,\n  \"allow_untested\": true\n}";
                File.WriteAllText(cfgPath, json, Encoding.UTF8);
            }
        }
        catch { }
    }

    private void CreateShortcut(string lnkPath, string targetExe, string workDir)
    {
        try
        {
            Type t = Type.GetTypeFromProgID("WScript.Shell");
            if (t == null) return;
            object shell = Activator.CreateInstance(t);
            object sc = t.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { lnkPath });
            Type st = sc.GetType();
            st.InvokeMember("TargetPath", BindingFlags.SetProperty, null, sc, new object[] { targetExe });
            st.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, sc, new object[] { workDir });
            st.InvokeMember("Description", BindingFlags.SetProperty, null, sc, new object[] { "FM27 Trainer by Holyman3" });
            st.InvokeMember("Save", BindingFlags.InvokeMethod, null, sc, null);
        }
        catch { }
    }

    private const string UNINSTALL_REG_KEY = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\FM27Trainer_Holyman3";

    private void RegisterUninstaller(string targetDir, string targetExe)
    {
        try
        {
            string uninstallerExe = Path.Combine(targetDir, "Uninstall FM27 Trainer.exe");
            try
            {
                string selfExe = Application.ExecutablePath;
                if (!string.Equals(Path.GetFullPath(selfExe), Path.GetFullPath(uninstallerExe), StringComparison.OrdinalIgnoreCase))
                {
                    File.Copy(selfExe, uninstallerExe, true);
                }
            }
            catch { }

            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(UNINSTALL_REG_KEY))
            {
                if (key != null)
                {
                    key.SetValue("DisplayName", "FM27 Trainer · Holyman3");
                    key.SetValue("DisplayVersion", BUNDLED_VERSION);
                    key.SetValue("Publisher", "Holyman3");
                    key.SetValue("DisplayIcon", "\"" + targetExe + "\",0");
                    key.SetValue("InstallLocation", targetDir);
                    key.SetValue("UninstallString", "\"" + uninstallerExe + "\" --uninstall");
                    key.SetValue("QuietUninstallString", "\"" + uninstallerExe + "\" --uninstall --quiet");
                    key.SetValue("URLInfoAbout", "https://github.com/Holymanrepo/Holyman3-Antreman-fm27");
                    key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                    key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    key.SetValue("EstimatedSize", 10400, RegistryValueKind.DWord);
                }
            }
        }
        catch { }
    }

    private static void RunUninstall(bool quiet)
    {
        try
        {
            if (!quiet)
            {
                DialogResult dr = MessageBox.Show(
                    "FM27 Trainer uygulamasını bilgisayarınızdan kaldırmak istediğinize emin misiniz?\n\nAre you sure you want to uninstall FM27 Trainer?",
                    "FM27 Trainer Kaldır / Uninstall",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Question
                );
                if (dr != DialogResult.Yes) return;
            }

            foreach (string name in new string[] { "FM27 Trainer", "fm27-trainer" })
            {
                foreach (Process p in Process.GetProcessesByName(name))
                {
                    try { p.Kill(); p.WaitForExit(1500); } catch { }
                }
            }

            string installDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "FM27 Trainer");
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(UNINSTALL_REG_KEY))
                {
                    if (key != null)
                    {
                        object loc = key.GetValue("InstallLocation");
                        if (loc != null && !string.IsNullOrEmpty(loc.ToString()))
                        {
                            installDir = loc.ToString();
                        }
                    }
                }
            }
            catch { }

            try
            {
                string deskLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), "FM27 Trainer.lnk");
                if (File.Exists(deskLnk)) File.Delete(deskLnk);
            }
            catch { }

            try
            {
                string startLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "FM27 Trainer.lnk");
                if (File.Exists(startLnk)) File.Delete(startLnk);
            }
            catch { }

            try
            {
                Registry.CurrentUser.DeleteSubKeyTree(UNINSTALL_REG_KEY, false);
            }
            catch { }

            try
            {
                string mainExe = Path.Combine(installDir, "FM27 Trainer.exe");
                if (File.Exists(mainExe)) File.Delete(mainExe);
                string oldExe = Path.Combine(installDir, "FM27 Trainer.exe.old");
                if (File.Exists(oldExe)) File.Delete(oldExe);
            }
            catch { }

            if (!quiet)
            {
                MessageBox.Show(
                    "FM27 Trainer başarıyla kaldırıldı.\n\nFM27 Trainer has been successfully uninstalled.",
                    "FM27 Trainer",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Information
                );
            }

            try
            {
                ProcessStartInfo psi = new ProcessStartInfo("cmd.exe", "/C ping 127.0.0.1 -n 2 > nul & rmdir /s /q \"" + installDir + "\"");
                psi.WindowStyle = ProcessWindowStyle.Hidden;
                psi.CreateNoWindow = true;
                Process.Start(psi);
            }
            catch { }
        }
        catch { }
    }

    [STAThread]
    public static void Main(string[] args)
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        if (args != null && args.Length > 0)
        {
            bool isUninstall = false;
            bool isQuiet = false;
            foreach (string a in args)
            {
                string low = a.ToLowerInvariant();
                if (low == "--uninstall" || low == "/uninstall" || low == "-uninstall") isUninstall = true;
                if (low == "--quiet" || low == "/quiet" || low == "/s") isQuiet = true;
            }
            if (isUninstall)
            {
                RunUninstall(isQuiet);
                return;
            }
        }
        Application.Run(new SetupForm());
    }
}
